import type { NearbyStore, Store } from '../../types';
import { distanceKm } from '../geo';

export interface AreaOptions {
	startKm: number;
	growth: number;
	maxKm: number;
	/** Grow until the circle holds at least this many stores… */
	minStores: number;
	/** …from at least this many different chains (or every chain that exists). */
	minChains: number;
	/** Never query more than this many stores of one chain, or in total. */
	maxPerChain: number;
	maxStores: number;
}

export const DEFAULT_AREA: AreaOptions = {
	startKm: 1.5,
	growth: 1.6,
	maxKm: 30,
	minStores: 12,
	minChains: 3,
	maxPerChain: 5,
	maxStores: 25
};

/**
 * Pick a search circle around `center` that is just big enough: dense cities get
 * a small circle, the countryside a large one.
 */
export function pickArea(center: { lat: number; lng: number }, all: Store[], opts = DEFAULT_AREA) {
	const sorted: NearbyStore[] = all
		.map((s) => ({ ...s, distanceKm: Math.round(distanceKm(center, s) * 100) / 100 }))
		.sort((a, b) => a.distanceKm - b.distanceKm);
	const minChains = Math.min(opts.minChains, new Set(all.map((s) => s.chain)).size);

	let radiusKm = opts.startKm;
	for (; radiusKm < opts.maxKm; radiusKm *= opts.growth) {
		const inside = sorted.filter((s) => s.distanceKm <= radiusKm);
		if (inside.length >= opts.minStores && new Set(inside.map((s) => s.chain)).size >= minChains) break;
	}
	radiusKm = Math.min(radiusKm, opts.maxKm);
	// Nothing within the cap (very remote): stretch to the single nearest store.
	if (sorted.length && sorted[0].distanceKm > radiusKm) radiusKm = sorted[0].distanceKm;
	radiusKm = Math.ceil(radiusKm * 10) / 10;

	const perChain = new Map<string, number>();
	const stores: NearbyStore[] = [];
	for (const s of sorted) {
		if (s.distanceKm > radiusKm || stores.length >= opts.maxStores) break;
		const n = perChain.get(s.chain) ?? 0;
		if (n >= opts.maxPerChain) continue;
		perChain.set(s.chain, n + 1);
		stores.push(s);
	}
	return { radiusKm, stores };
}
