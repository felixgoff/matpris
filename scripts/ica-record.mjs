// Records the shop's own API traffic while you click around by hand — the fastest
// way to learn how search, categories and product pages really talk to their back end.
//
// Run: bun run record:ica [accountId]
// Then browse like a human: type a search, open categories, click products. Every
// /api/ request and JSON response is copied to the terminal and appended to
// scripts/.ica-profile/journey.jsonl. Stop by closing the browser window.
// (The profile dir is shared with the crawler — run them one at a time.)
import { appendFile, mkdir } from 'node:fs/promises';
import { chromium } from 'playwright';

const accountId = process.argv[2] ?? '1088004'; // Maxi ICA Stormarknad Gunnesbo, Lund
const BASE = 'https://handlaprivatkund.ica.se';
const LOG = new URL('./.ica-profile/journey.jsonl', import.meta.url);

await mkdir(new URL('./.ica-profile/', import.meta.url), { recursive: true });

const log = (...args) => console.log(new Date().toLocaleTimeString('sv-SE'), ...args);

const ctx = await chromium.launchPersistentContext(new URL('./.ica-profile/', import.meta.url).pathname, {
	headless: false,
	channel: 'chrome',
	args: ['--disable-blink-features=AutomationControlled'],
	locale: 'sv-SE',
	viewport: { width: 1440, height: 900 }
});
const page = ctx.pages()[0] ?? (await ctx.newPage());
log('recording — browse the shop by hand; closing the window ends the recording');

page.on('framenavigated', async (f) => {
	if (f !== page.mainFrame()) return;
	await appendLine({ t: Date.now(), kind: 'page', url: f.url() });
});

ctx.on('response', async (res) => {
	const u = new URL(res.url());
	if (!/\/api\//.test(u.pathname)) return;
	const req = res.request();
	const entry = {
		t: Date.now(),
		kind: 'api',
		method: req.method(),
		url: res.url(),
		query: Object.fromEntries(u.searchParams),
		body: req.postData() ?? null,
		status: res.status(),
		// First chunk of JSON responses: response bodies show the data shape.
		resp: (await res.text().catch(() => '')).slice(0, 4_000)
	};
	await appendLine(entry);
});

async function appendLine(entry) {
	const summary =
		entry.kind === 'api'
			? `${entry.method} ${new URL(entry.url).pathname} [${entry.status}]${entry.body ? ` body ${entry.body.length}B` : ''}`
			: `page ${entry.url}`;
	log(summary);
	await appendFile(LOG, `${JSON.stringify(entry)}\n`);
}

ctx.on('close', () => process.exit(0));
