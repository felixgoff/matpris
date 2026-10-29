// Builds src/lib/server/data/ica-catalogue.json: the full priced assortment of one
// ICA store (default: Maxi ICA Stormarknad Gunnesbo, Lund), crawled like a shopper:
// category pages are opened in a real, visible Chrome window (Playwright) and products
// are harvested from the JSON the site's own web app fetches (webproductpagews batch
// endpoints).
//
// Why not plain HTTP: handlaprivatkund.ica.se sits behind AWS WAF. A Playwright-minted
// token does not carry over to server-side HTTP, and the search journey shows a visual
// CAPTCHA. In-browser category browsing from a residential IP at shopper pacing passes
// without any challenge (matval crawled 33k pages this way, zero blocks).
//
// About the CAPTCHA: this crawler never defeats it. It runs a real browser window that
// keeps a persistent profile (so session and WAF tokens accumulate), starts with a
// warm-up so the passive JS challenge can solve itself, and if a visual CAPTCHA does
// appear it stops and waits — you solve the puzzle by hand in the window, like ICA
// intends a human to, and the crawl resumes where it stopped. Solving services or OCR
// are out of scope on purpose; getting flagged for them means harder challenges and no
// data at all.
//
// Resumable: categories and products already in the output file are skipped, so
// re-running fills gaps without re-crawling everything. Full crawl ~30-45 min.
// Run: bun run catalogue:ica [accountId]
// Env: HEADLESS=1 run without a window (challenge then aborts instead of waiting);
//      MAX_PAGES=10 for a short test run.
import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import { chromium } from 'playwright';

const accountId = process.argv[2] ?? '1088004'; // Maxi ICA Stormarknad Gunnesbo, Lund
const BASE = 'https://handlaprivatkund.ica.se';
const OUT = new URL('../src/lib/server/data/ica-catalogue.json', import.meta.url);
const TMP = new URL('./.ica-profile/catalogue.tmp.json', import.meta.url);
const HEADLESS = process.env.HEADLESS === '1';
const MAX_PAGES = Number(process.env.MAX_PAGES ?? 0); // 0 = unlimited

// Categories without food prices — crawled last.
const LOW_PRIORITY = ['apotek', 'traning', 'kok', 'hem', 'fritid', 'blommor', 'tobak', 'djur', 'stad'];

// The WAF's challenge texts (the escalation switched ours from Swedish to English once).
const CHALLENGE_MARKERS = [
	'Bekräfta att du är en människa', // sv: "confirm you are human"
	'Bekräfta att du är en människa',
	"Let's confirm you are human",
	'aws-waf-token' // challenge page scripts always reference it
];
const CHALLENGE_TIMEOUT_MIN = 10; // minutes to wait for a human solve before giving up

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (...args) => console.log(new Date().toLocaleTimeString('sv-SE'), ...args);

let db;
try {
	db = JSON.parse(await readFile(OUT, 'utf8'));
} catch {
	db = { stores: {}, products: [] };
}
const storeState = (db.stores[accountId] ??= { categories: {}, visited: {} });

// ---------- launch and session ----------
let browser, ctx, page;

/** Real Chrome if installed (best fingerprint), else the bundled Chromium. */
async function launch() {
	const profile = new URL('./.ica-profile', import.meta.url);
	await mkdir(profile, { recursive: true });
	for (const opts of [{ channel: 'chrome' }, {}]) {
		try {
			browser = await chromium.launch({
				headless: HEADLESS,
				...(opts.channel ? opts : {}),
				args: ['--disable-blink-features=AutomationControlled']
			});
			log(`browser: ${opts.channel ?? 'bundled chromium'}${HEADLESS ? ' (headless)' : ' (headed)'}`);
			return;
		} catch (e) {
			log(`no ${opts.channel ?? 'chromium'}: ${String(e.message).split('\n')[0]}`);
		}
	}
	throw new Error('no browser could be launched');
}

let consentDone = false;

async function closeConsent() {
	if (!consentDone) {
		await page.evaluate(() => {
			const el = document.getElementById('onetrust-consent-sdk');
			if (el) el.style.display = 'none';
		});
		consentDone = true;
	}
}

/** The site's challenge is a page/iframe asking a human to prove themselves. */
async function isChallenge() {
	const content = await page.content();
	if (CHALLENGE_MARKERS.some((m) => content.includes(m))) return true;
	// The visual puzzle lives in a hosted iframe.
	return page.frames().some((f) => /awswaf|captcha/i.test(f.url()) && f !== page.mainFrame());
}

const hasWafToken = async () => (await ctx.cookies()).some((c) => c.name === 'aws-waf-token');

/**
 * A challenge appeared. Do NOT solve it programmatically: show it to the human and
 * wait for them to complete it in the window. Returns once a fresh token exists.
 */
async function waitOutChallenge(url) {
	if (HEADLESS) throw new Error('CAPTCHA in headless mode — re-run headed (default) and click it');
	log('=== CAPTCHA — solve it in the browser window, I will resume this page when you do ===');
	for (let i = 0; i < (CHALLENGE_TIMEOUT_MIN * 60) / 5; i++) {
		process.stdout.write('\x07'); // bell
		await sleep(5_000);
		if (await hasWafToken()) {
			log('WAF token minted — reloading the page');
			await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 60_000 });
			await page.waitForTimeout(4_000);
			if (await isChallenge()) continue; // the solve didn't take; wait again
			return true;
		}
	}
	throw new Error('no human solve within the timeout');
}

/** Navigate and clear any challenge (human-solved). Retries a few times per URL. */
async function safeGoto(url) {
	let strikes = 0;
	for (;;) {
		await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 60_000 });
		await page.waitForTimeout(2_000);
		await closeConsent();
		if (await isChallenge()) {
			if (++strikes >= 3) throw new Error('the WAF keeps challenging even after solving');
			await waitOutChallenge(url);
			continue;
		}
		// Give the passive JS challenge a moment to mint a token even if the page loads.
		if (!(await hasWafToken())) await sleep(3_000);
		return;
	}
}

// ---------- data harvesting ----------
const slug = (s = '') =>
	s
		.normalize('NFKD')
		.replace(/[^\w\s-]/g, '')
		.trim()
		.replace(/[\s_]+/g, '-')
		.toLowerCase();

function keepPrice(id, name, values) {
	const { price, unitPrice, unit, size, available, ean, cats, retailerId } = values;
	if (!price) return;
	db.products = db.products.filter((x) => x.id !== id);
	db.products.push({
		id,
		name,
		brand: values.brand,
		price,
		unitPrice,
		unit,
		size,
		available: !!available,
		ean,
		categories: cats?.slice(0, 2),
		url: `${BASE}/stores/${accountId}/products/${slug(name)}/${retailerId ?? ''}`
	});
}

/** Harvest response bodies of the SPA's own API calls (verified 2026-10-29 with matval). */
const handlers = [
	{
		marker: 'webproductpagews/v6/products',
		apply(p) {
			for (const x of p?.products ?? [])
				keepPrice(String(x.productId ?? ''), x.name, {
					price: x.price?.amount != null ? Number(x.price.amount) : undefined,
					unitPrice: x.unitPrice?.amount != null ? Number(x.unitPrice.amount) : undefined,
					unit: x.unitPrice?.label,
					size: x.packSizeDescription ?? x.size?.value,
					available: x.available,
					ean: x.ean,
					cats: x.categoryPath,
					retailerId: x.retailerProductId,
					brand: x.manufacturer
				});
		}
	},
	{
		marker: 'webproductpagews/v5/product-pages',
		apply(p) {
			for (const x of p?.productPages ?? p?.products ?? []) {
				const price = x.price?.current ?? x.price ?? {};
				keepPrice(String(x.productId ?? x.id ?? ''), x.name, {
					price: price.amount != null ? Number(price.amount) : undefined,
					unitPrice: x.unitPrice?.current?.amount ?? x.unitPrice?.amount,
					unit: x.unitPrice?.current?.label ?? x.unitPrice?.label,
					size: x.packSizeDescription ?? x.size?.value,
					available: x.available,
					ean: x.ean,
					cats: x.categoryPath,
					retailerId: x.retailerProductId,
					brand: x.manufacturer
				});
			}
		}
	}
];

let saves = 0;

/** Write the .tmp file and atomically replace the output. */
async function flush() {
	await writeFile(TMP, JSON.stringify(db));
	await rename(TMP.pathname, OUT.pathname);
}

async function flushSoon() {
	if (++saves % 5 === 0) await flush();
}

function attachHarvester() {
	page.on('response', async (r) => {
		const u = new URL(r.url());
		const h = handlers.find((x) => u.pathname.includes(x.marker));
		if (!h) return;
		try {
			const ct = r.headers()['content-type'] ?? '';
			if (!ct.includes('json')) return;
			await h.apply(await r.json());
		} catch {
			/* ignore */
		}
	});
}

async function discoverSubcategories() {
	return page.$$eval('a[href*="/categories/"]', (as) =>
		as.map((a) => a.getAttribute('href')).filter((u) => u && /\/stores\/\w+\/categories\/.+/.test(u))
	);
}

async function visitCategory(href) {
	const url = href.startsWith('http') ? href : `${BASE}${href}`;
	const slugKey = decodeURIComponent(href.split('/categories/')[1] ?? href).split('?')[0];

	await safeGoto(url);

	// Scroll to the bottom in steps so the app fetches every product batch.
	let lastHeight = -1;
	for (let i = 0; i < 25; i++) {
		await page.mouse.wheel(0, 4_000);
		await sleep(1_500);
		const h = await page.evaluate(() => document.body.scrollHeight);
		if (h === lastHeight) {
			// two extra attempts in case a render lagged
			await page.mouse.wheel(0, 8_000);
			await sleep(2_000);
			if ((await page.evaluate(() => document.body.scrollHeight)) === lastHeight) break;
			log('  growth…');
		}
		lastHeight = h;
	}

	storeState.visited[slugKey] = Date.now();
	for (const sub of await discoverSubcategories()) {
		const key = decodeURIComponent(sub.split('/categories/')[1] ?? sub).split('?')[0];
		if (!storeState.categories[key]) storeState.categories[key] = { href: sub, depth: slugKey.split('/').length };
	}
	await flushSoon();
}

// ---------- category queue ----------
const seededRoots = (slugify = false) => [
	['Frukt & Grönt', 'Frukt-Grönt', '03968fc5-dadb-4e2b-8983-6abbf641df3c'],
	['Kött, Chark & Fågel', 'Kott-Chark-Fagel', '4d07744d-fd8d-47ea-89e6-38c49ca44652'],
	['Fisk & Skaldjur', 'Fisk-Skaldjur', '3bfbe616-f05c-4fdf-823a-f55ed6eed6c2'],
	['Mejeri & Ost', 'Mejeri-Ost', '03d68f50-5a8c-4b9c-95a1-f0f017cacab0'],
	['Bröd & Kakor', 'Brod-Kakor', 'c7739997-6b40-45c9-9042-a6102ae9779c'],
	['Vegetariskt', 'Vegetariskt', '0b6beda8-526a-49c5-b533-cb3b8474f3b3'],
	['Färdigmat', 'Fardigmat', '67062250-87a0-4b75-be6c-21413a477e79'],
	['Barn', 'Barn', '3f7fdab0-b5c9-451b-b081-98f7b6f01d82'],
	['Glass, Godis & Snacks', 'Glass-Godis-Snacks', '0053d478-6e25-4982-aa2c-ea5e5770a071'],
	['Dryck', 'Dryck', '7a765e3c-d8a5-4f1d-afa3-93761d10f3c1'],
	['Skafferi', 'Skafferi', '31c18410-0856-4908-8834-1eea8808c498'],
	['Fryst', 'Fryst', '3937612b-efec-4ede-91ae-57904b8473aa'],
	['Skönhet', 'Skonhet', '8a38226b-8bba-4905-8ed3-bb28e32eadf5'],
	['Träning & Hälsa', 'Traning-Halsa', 'e89c368d-4d41-4086-9802-90a13490bac8'],
	['Djur', 'Djur', '42388d25-26a7-40f5-ac5d-7a65c8da784f'],
	['Städ, Tvätt & Papper', 'Stad-Tvatt-Papper', '978ea4a6-5267-4fb7-a474-67e5dceeb3c9'],
	['Kök', 'Kok', '9d39ff06-9c72-46c5-a69e-dbaa2fab7411'],
	['Hem & Inredning', 'Hem-Inredning', '2b2f384d-2caa-43f3-ad8e-403b1a7be4e5'],
	['Fritid', 'Fritid', '665217df-7775-4b3b-980b-3e094003a5a1'],
	['Blommor & Trädgård', 'Blommor-Tradgard', 'cae1e58c-a558-4eff-a899-7ece0ce575f9'],
	['Tobak', 'Tobak', '331db70a-9d3f-4574-96fc-3543d7149a57']
];

function seedRoots() {
	if (Object.keys(storeState.categories).length) return; // resuming: keep discovered list
	for (const [, path, id] of seededRoots())
		storeState.categories[`${path}/${id}`] = {
			href: `/stores/${accountId}/categories/${path}/${id}`,
			depth: 1
		};
}

function selectNext() {
	const entries = Object.entries(storeState.categories).filter(([k]) => !storeState.visited[k]);
	entries.sort((a, b) => {
		const pa = LOW_PRIORITY.some((p) => a[0].toLowerCase().startsWith(p)) || (a[1].depth ?? 1) > 2 ? 1 : 0;
		const pb = LOW_PRIORITY.some((p) => b[0].toLowerCase().startsWith(p)) || (b[1].depth ?? 1) > 2 ? 1 : 0;
		if (pa !== pb) return pa - pb;
		return (a[1].depth ?? 1) - (b[1].depth ?? 1);
	});
	return entries[0];
}

// ---------- main ----------
await launch();
ctx = await browser.newContext({
	userAgent:
		'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36',
	locale: 'sv-SE',
	viewport: { width: 1440, height: 900 }
});
page = await ctx.newPage();
attachHarvester();
seedRoots();

if (MAX_PAGES === 0) log(`${Object.keys(storeState.visited).length} visited, ${db.products.length} priced products in existing index`);

// Warm-up: the store's own landing page mints the passive WAF token before we crawl.
await safeGoto(`${BASE}/stores/${accountId}`);
if (!(await hasWafToken())) {
	for (let i = 0; i < 20 && !(await hasWafToken()); i++) await sleep(1_000);
}

let done = 0;
try {
	for (;;) {
		const [key, cat] = selectNext() ?? [];
		if (!key) break;
		try {
			await visitCategory(cat.href);
			log(`[${++done}${MAX_PAGES ? `/${MAX_PAGES}` : ''}] ${key}`);
			if (MAX_PAGES && done >= MAX_PAGES) break;
			await sleep(2_500 + Math.random() * 2_500); // shopper pacing
		} catch (e) {
			console.error(`  failed: ${e.message.split('\n')[0]}`);
			if (/CAPTCHA|no human solve|keeps challenging/.test(e.message)) break; // reputation came back; retry later
		}
	}
} finally {
	await flush();
}

db = JSON.parse(await readFile(OUT, 'utf8'));
log(
	`done: ${done + Object.keys(storeState.visited).length} categories visited total, ` +
		`${db.products.reduce((n, p) => n + !!p.price, 0)} priced products`
);
await browser.close();
