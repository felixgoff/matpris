// Builds src/lib/server/data/{willys,hemkop}-stores.json from Axfood's store endpoint,
// so searches never spend a site's one-request-per-10-s turn on the store list.
// One request per site. Re-run occasionally (stores rarely change): bun run stores:axfood
import { writeFile } from 'node:fs/promises';

const UA = 'matpris/0.1 (price comparison; store list builder)';
const SITES = { willys: 'https://www.willys.se', hemkop: 'https://www.hemkop.se' };

for (const [chain, base] of Object.entries(SITES)) {
	const res = await fetch(`${base}/axfood/rest/store?online=false`, { headers: { 'user-agent': UA } });
	if (!res.ok) throw new Error(`${chain}: ${res.status}`);
	const stores = (await res.json())
		.filter((s) => s.geoPoint?.latitude && s.name)
		.map((s) => ({
			id: s.storeId,
			chain,
			name: s.name,
			lat: s.geoPoint.latitude,
			lng: s.geoPoint.longitude,
			address: s.address?.formattedAddress ?? ''
		}));
	const out = new URL(`../src/lib/server/data/${chain}-stores.json`, import.meta.url);
	await writeFile(out, JSON.stringify(stores, null, '\t') + '\n');
	console.log(`${chain}: ${stores.length} stores`);
}
