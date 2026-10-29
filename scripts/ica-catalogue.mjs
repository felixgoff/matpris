// Builds src/lib/server/data/ica-catalogue.json: the full priced assortment of one
// ICA store (default: Maxi ICA Stormarknad Gunnesbo, Lund), crawled like a shopper:
// category pages are opened in Chromium (Playwright), products are harvested from
// the JSON the site's own web app fetches (webproductpagews batch endpoints).
//
// Why not plain HTTP: handlaprivatkund.ica.se sits behind AWS WAF. A Playwright-
// minted token does not carry over to server-side HTTP (curl gets a CAPTCHA), and
// the search journey triggers a visual CAPTCHA outright. In-browser category browsing
// works reliably, so the crawl uses exactly that; a visual CAPTCHA aborts the crawl
// instead of being solved.
//
// Resumable: categories and products already in the output file are skipped, so
// re-running fills gaps without re-crawling everything. Runtime ~30-45 min.
// Run: bun run catalogue:ica [accountId]
import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import { chromium } from 'playwright';

const accountId = process.argv[2] ?? '1088004'; // Maxi ICA Stormarknad Gunnesbo, Lund
const BASE = 'https://handlaprivatkund.ica.se';
const OUT = new URL('../src/lib/server/data/ica-catalogue.json', import.meta.url);
const TMP = new URL('../src/lib/server/data/.ica-catalogue.tmp.json', import.meta.url);

// Categories without food prices (pharmacy & durable goods) — crawled last, if at all.
const LOW_PRIORITY = ['apotek', 'traning', 'kok', 'hem', 'fritid', 'blommor', 'tobak', 'djur', 'stad'];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let db;
try {
	db = JSON.parse(await readFile(OUT, 'utf8'));
} catch {
	db = { stores: {}, categories: {}, products: [] };
}
const storeState = (db.stores[accountId] ??= { categories: {}, visited: {} });

let browser, ctx, page;
let cookieConsentDone = false;

async function wafTokenReady() {
	for (let i = 0; i < 40; i++) {
		for (const c of await ctx.cookies()) if (c.name === 'aws-waf-token') return true;
		await sleep(500);
	}
	return false;
}

const seenCaptcha = async () =>
	(await page.content()).includes('Bekräfta att du är en människa') || (await page.$('iframe[title*="challenge" i], iframe[src*="awswaf"]')) !== null;

// Product JSON, as harvested from the site's own API responses.
const handlers = {
	// PUT batch: { products: [{ productId, name, price: {amount}, unitPrice: {amount,label}, offers… }] }
	async 'webproductpagews/v6/products' (payload) {
		for (const p of payload?.products ?? []) {
			const id = String(p.productId ?? '');
			if (!id || !p.name) continue;
			db.products = db.products.filter((x) => x.id !== id);
			db.products.push({
				id,
				name: p.name,
				brand: p.manufacturer ?? undefined,
				price: p.price?.amount != null ? Number(p.price.amount) : undefined,
				unitPrice: p.unitPrice?.amount != null ? Number(p.unitPrice.amount) : undefined,
				unit: p.unitPrice?.label ?? undefined,
				size: p.packSizeDescription ?? p.size?.value ?? undefined,
				available: !!p.available,
				ean: p.ean ?? undefined,
				categories: (p.categoryPath ?? []).slice(0, 2),
				url: `${BASE}/stores/${accountId}/products/${slug(p.name)}/${p.retailerProductId ?? ''}`
			});
		}
	},
	// GET product pages (v5): paginated catalogue listings
	async 'webproductpagews/v5/product-pages' (payload) {
		for (const p of payload?.productPages ?? payload?.products ?? []) {
			const price = p.price?.current ?? p.price ?? {};
			const id = String(p.productId ?? p.id ?? '');
			if (!id || !p.name || price.amount == null) continue;
			db.products = db.products.filter((x) => x.id !== id);
			db.products.push({
				id,
				name: p.name,
				brand: p.manufacturer ?? undefined,
				price: Number(price.amount),
				unitPrice: p.unitPrice?.current?.amount != null ? Number(p.unitPrice.current.amount) : p.unitPrice?.amount,
				unit: p.unitPrice?.current?.label ?? p.unitPrice?.label ?? undefined,
				size: p.packSizeDescription ?? p.size?.value ?? undefined,
				available: !!p.available,
				ean: p.ean ?? undefined,
				categories: (p.categoryPath ?? []).slice(0, 2),
				url: `${BASE}/stores/${accountId}/products/${slug(p.name)}/${p.retailerProductId ?? ''}`
			});
		}
	}
};

const slug = (s = '') =>
	s
		.normalize('NFKD')
		.replace(/[^\w\s-]/g, '')
		.trim()
		.replace(/[\s_]+/g, '-')
		.toLowerCase();

let saves = 0;

const tmpPathname = TMP.pathname;
const outPathname = OUT.pathname;

/** Write the .tmp file and atomically replace the output. */
async function flush() {
	await writeFile(TMP, JSON.stringify(db));
	await rename(tmpPathname, outPathname);
}

async function flushSoon() {
	saves++;
	if (saves % 5 === 0) await flush();
}
// Harvest API responses on every page.
function attachHarvester() {
	page.on('response', async (r) => {
		const u = new URL(r.url());
		for (const [marker, extract] of Object.entries(handlers)) {
			if (u.pathname.includes(marker) && (r.request().method() === 'PUT' || r.status() === 200)) {
				try {
					const ct = r.headers()['content-type'] || '';
					if (!ct.includes('json')) return;
					await extract(await r.json());
				} catch {
					/* ignore */
				}
			}
		}
	});
}

/** Discover subcategory links on a category page. */
async function discoverSubcategories() {
	return page.$$eval('a[href*="/categories/"]', (as) =>
		as.map((a) => a.getAttribute('href')).filter((u) => u && /\/stores\/\w+\/categories\/.+/.test(u))
	);
}

async function visitCategory(href, index, total) {
	const url = href.startsWith('http') ? href : `${BASE}${href}`;
	const slugKey = decodeURIComponent(href.split('/categories/')[1] ?? href).split('?')[0];
	if (storeState.visited[slugKey]) return false;

	console.log(`[${index}/${total}] ${slugKey}`);
	await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 60_000 });
	await page.waitForTimeout(1_500);

	if (!(await wafTokenReady())) console.warn('  no WAF token yet — continuing anyway');

	if (await seenCaptcha()) {
		console.error(`CAPTCHA at ${url}, aborting crawl (state saved)`);
		await flush();
		process.exit(2);
	}

	if (!cookieConsentDone) {
		await page.evaluate(() => {
			const el = document.getElementById('onetrust-consent-sdk');
			if (el) el.style.display = 'none';
		});
		cookieConsentDone = true;
	}

	// Scroll to the bottom in steps so the app fetches every product batch.
	let lastHeight = -1;
	for (let i = 0; i < 25; i++) {
		await page.mouse.wheel(0, 4_000);
		await sleep(1_200);
		const h = await page.evaluate(() => document.body.scrollHeight);
		if (h === lastHeight) {
			// two extra attempts in case a render lagged
			await page.mouse.wheel(0, 8_000);
			await sleep(1_800);
			const h2 = await page.evaluate(() => document.body.scrollHeight);
			if (h2 === lastHeight) break;
		}
		lastHeight = h;
	}

	storeState.visited[slugKey] = Date.now();
	for (const sub of await discoverSubcategories()) {
		const key = decodeURIComponent(sub.split('/categories/')[1] ?? sub).split('?')[0];
		if (!storeState.categories[key]) storeState.categories[key] = { href: sub, depth: slugKey.split('/').length };
	}
	await flushSoon();
	return true;
}

// ---- main ----
browser = await chromium.launch({ headless: true });
ctx = await browser.newContext({
	userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36',
	locale: 'sv-SE'
});
page = await ctx.newPage();
attachHarvester();

// Seed: the site's root categories (from matval's mapping, they are the national tree).
const ROOTS = [
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
for (const [name, path, id] of ROOTS) {
	const key = `${path}/${id}`;
	if (!storeState.categories[key]) storeState.categories[key] = { href: `/stores/${accountId}/categories/${path}/${id}`, depth: 1 };
}

// Breadth-first-ish: process the queue; deeper and low-priority categories go last.
const selectNext = () => {
	const entries = Object.entries(storeState.categories).filter(([k]) => !storeState.visited[k]);
	entries.sort((a, b) => {
		const pa = LOW_PRIORITY.some((p) => a[0].toLowerCase().startsWith(p)) || (a[1].depth ?? 1) > 2 ? 1 : 0;
		const pb = LOW_PRIORITY.some((p) => b[0].toLowerCase().startsWith(p)) || (b[1].depth ?? 1) > 2 ? 1 : 0;
		if (pa !== pb) return pa - pb;
		return (a[1].depth ?? 1) - (b[1].depth ?? 1);
	});
	return entries[0];
};

let done = 0;
for (;;) {
	const [key, cat] = selectNext() ?? [];
	if (!key) break;
	try {
		if (await visitCategory(cat.href, ++done, done + Object.values(storeState.visited).length)) {
			await sleep(1_500 + Math.random() * 1_500);
		}
	} catch (e) {
		console.warn(`  failed: ${e.message.split('\n')[0]}`);
	}
}
await flush();
const ymd = new Date().toISOString().slice(0, 10);
db = JSON.parse(await readFile(OUT, 'utf8'));
console.log(`done: ${done} categories this run, ${db.products.length} priced products, ${Object.keys(storeState.visited).length} categories visited`);
