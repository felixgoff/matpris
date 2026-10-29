// Store locations for chains without public price data (ICA, Coop), from
// OpenStreetMap via Nominatim. Usage policy: max 1 request/s, identify yourself,
// no heavy use — so lookups are per search circle and cached for a day.
import type { ChainId, Store } from '../../types';
import { CHAINS } from '../../types';
import { TtlCache } from '../cache';
import { fetchJson } from '../http';

interface NominatimPlace {
	osm_id: number;
	name: string;
	lat: string;
	lon: string;
	category: string;
	type: string;
	display_name: string;
}

const STORE_TYPES = new Set(['supermarket', 'convenience']);

/** "ICA Malmborgs, 5, Clemenstorget, Stadskärnan, …" → "Clemenstorget 5, Stadskärnan" */
function address(displayName: string) {
	const parts = displayName.split(', ').slice(1);
	if (/^\d+\w?(-\d+)?$/.test(parts[0] ?? '')) parts.splice(0, 2, `${parts[1]} ${parts[0]}`);
	return parts.slice(0, 2).join(', ');
}
const cache = new TtlCache<Store[]>(24 * 60 * 60 * 1000);

/** ICA or Coop grocery stores inside the box around `center` (≈ `radiusKm` each way). */
export function osmStores(chain: ChainId, center: { lat: number; lng: number }, radiusKm: number): Promise<Store[]> {
	// Round so nearby searches share a cache entry.
	const key = `${chain}:${center.lat.toFixed(2)}:${center.lng.toFixed(2)}:${Math.ceil(radiusKm)}`;
	return cache.get(key, async () => {
		const dLat = radiusKm / 111;
		const dLng = radiusKm / (111 * Math.cos((center.lat * Math.PI) / 180));
		const viewbox = [center.lng - dLng, center.lat + dLat, center.lng + dLng, center.lat - dLat].join(',');
		const brand = CHAINS[chain].name;
		const places = await fetchJson<NominatimPlace[]>(
			`https://nominatim.openstreetmap.org/search?format=jsonv2&layer=poi&bounded=1&limit=40` +
				`&q=${encodeURIComponent(brand)}&viewbox=${viewbox}`,
			{ intervalMs: 1100 }
		);
		const brandWord = new RegExp(`(^|\\s)${brand}(\\s|$)`, 'i');
		const seen = new Set<string>();
		const stores: Store[] = [];
		for (const p of places) {
			if (p.category !== 'shop' || !STORE_TYPES.has(p.type) || !brandWord.test(p.name)) continue;
			const lat = Number(p.lat);
			const lng = Number(p.lon);
			// The same store is sometimes mapped twice (node + building).
			const dedupe = `${p.name}:${lat.toFixed(3)}:${lng.toFixed(3)}`;
			if (seen.has(dedupe)) continue;
			seen.add(dedupe);
			stores.push({
				id: `osm-${p.osm_id}`,
				chain,
				name: p.name,
				lat,
				lng,
				address: address(p.display_name)
			});
		}
		return stores;
	});
}
