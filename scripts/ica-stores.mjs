// Builds core/data/ica-stores.json from ICA's store selector.
// handla.ima.se's selector app loads all stores from
//   GET https://handla.ica.se/api/store/v1?groupby=citygroup&customerType=B2C
// which sits behind the same AWS WAF as the rest of ICA, so it is fetched once
// from a real Chromium page (Playwright) instead of plain HTTP.
// Re-run occasionally (stores rarely change): bun run stores:ica
import { mkdir, writeFile } from 'node:fs/promises';
import { chromium } from 'playwright';

const OUT = new URL('../core/data/ica-stores.json', import.meta.url);

const browser = await chromium.launch({ headless: true });
const ctx = await browser.newContext({
	userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36',
	locale: 'sv-SE'
});
const page = await ctx.newPage();

let raw = null;
page.on('response', async (r) => {
	if (/\/api\/store\/v1/.test(new URL(r.url()).pathname) && r.status() === 200 && !raw) {
		try {
			raw = await r.json();
		} catch {
			/* ignore */
		}
	}
});

await page.goto('https://handla.ica.se/?chooseStore=true', { waitUntil: 'domcontentloaded', timeout: 60_000 });
await page.waitForTimeout(6_000);
const decline = await page.$('button:has-text("Avvisa alla")');
if (decline) {
	await decline.click();
	await page.waitForTimeout(2_000);
}
const picker = await page.$('text=Hitta butik efter ort');
if (picker) {
	await picker.click();
	await page.waitForTimeout(7_000);
}
if (!raw) throw new Error('store list API was not observed — check the store selector page');
await browser.close();

// {A: {Alingsås: [store, …]}, …} → flat list, deduped by accountId.
const byId = new Map();
const walk = (o) => {
	if (Array.isArray(o)) for (const x of o) walk(x);
	else if (o && typeof o === 'object') {
		if ('accountId' in o && 'latitude' in o) byId.set(o.accountId, o);
		else for (const v of Object.values(o)) walk(v);
	}
};
walk(raw);

const stores = [...byId.values()]
	.filter((s) => s.enable === '1' && s.published === '1' && s.customerTypes?.includes('B2C') && s.latitude)
	.map((s) => ({
		id: s.accountId,
		chain: 'ica',
		name: s.name,
		lat: s.latitude,
		lng: s.longitude,
		address: [s.street, s.zipCode, s.city].filter(Boolean).join(', '),
		url: `https://handlaprivatkund.ica.se/stores/${s.accountId}`
	}))
	.sort((a, b) => a.name.localeCompare(b.name, 'sv'));

await mkdir(new URL('.', OUT), { recursive: true });
await writeFile(OUT, JSON.stringify(stores, null, '\t') + '\n');
console.log(`wrote ${stores.length} ICA stores`);
