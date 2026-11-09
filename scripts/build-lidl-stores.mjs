// Builds core/data/lidl-stores.json from Lidl's public store sitemap.
// Each store page embeds schema.org JSON-LD with address + coordinates.
// Runs politely: one request at a time, ~1 req/s. Re-run occasionally (stores rarely change).
import { writeFile, mkdir } from 'node:fs/promises';

const UA = 'matpris/0.1 (price comparison; store list builder)';
const OUT = new URL('../core/data/lidl-stores.json', import.meta.url);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function get(url) {
	const res = await fetch(url, { headers: { 'user-agent': UA } });
	if (!res.ok) throw new Error(`${res.status} ${url}`);
	return res.text();
}

const sitemap = await get('https://www.lidl.se/s/sv-SE/butiker/sitemap.xml');
// Store pages are /butiker/<town>/<street>/ — town-only pages are listings, skip them.
const urls = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)]
	.map((m) => m[1])
	.filter((u) => /\/butiker\/[^/]+\/[^/]+\/$/.test(u));

console.log(`${urls.length} store pages`);
const stores = [];
for (const [i, url] of urls.entries()) {
	try {
		const html = await get(url);
		for (const m of html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)) {
			const ld = JSON.parse(m[1]);
			if (ld['@type'] !== 'GroceryStore' || !ld.geo) continue;
			const a = ld.address ?? {};
			stores.push({
				id: url.split('/butiker/')[1].replace(/\/$/, ''),
				chain: 'lidl',
				name: ld.name.replace(/^Lidl - /, 'Lidl '),
				lat: ld.geo.latitude,
				lng: ld.geo.longitude,
				address: [a.streetAddress, [a.postalCode, a.addressLocality].filter(Boolean).join(' ')]
					.filter(Boolean)
					.join(', '),
				url
			});
		}
	} catch (e) {
		console.warn(`skip ${url}: ${e.message}`);
	}
	if (i % 25 === 0) console.log(`${i}/${urls.length}`);
	await sleep(1000);
}

await mkdir(new URL('.', OUT), { recursive: true });
await writeFile(OUT, JSON.stringify(stores, null, '\t') + '\n');
console.log(`wrote ${stores.length} stores`);
