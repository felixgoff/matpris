import type { ChainId, NearbyStore, Store } from '../../types';
import { CHAINS } from '../../types';
import { TtlCache } from '../cache';
import { adapters } from '../chains';
import { distanceKm } from '../../geo';
import { osmStores } from './osm';
import { DEFAULT_AREA, pickArea } from './radius';

const DAY = 24 * 60 * 60 * 1000;
const cache = new TtlCache<Store[]>(DAY);

/** Every known store of every chain with prices. A chain whose store list fails is skipped. */
export function allStores(): Promise<Store[]> {
	return cache.get('all', async () => {
		const lists = await Promise.all(
			Object.values(adapters).map((a) =>
				a.listStores().catch((e) => {
					console.warn(`[stores] ${a.chain}: ${e.message}`);
					return [] as Store[];
				})
			)
		);
		const stores = lists.flat();
		if (!stores.length) throw new Error('no store lists available');
		return stores;
	});
}

export type Area = { radiusKm: number; stores: NearbyStore[] };

/** The search circle, sized by the stores that have prices. */
export async function pricedArea(center: { lat: number; lng: number }): Promise<Area> {
	return pickArea(center, await allStores());
}

export const MAP_ONLY_CHAINS = (Object.keys(CHAINS) as ChainId[]).filter((c) => !CHAINS[c].online);

/** Adds the nearest stores of chains without online prices (ICA, Coop) inside the same circle. */
export async function withMapOnlyStores(center: { lat: number; lng: number }, area: Area): Promise<Area> {
	const lists = await Promise.all(
		MAP_ONLY_CHAINS.map((chain) =>
			osmStores(chain, center, area.radiusKm).catch((e) => {
				console.warn(`[stores] ${chain}: ${e.message}`);
				return [] as Store[];
			})
		)
	);
	const extra = lists.flatMap((stores) =>
		stores
			.map((s) => ({ ...s, distanceKm: Math.round(distanceKm(center, s) * 100) / 100 }))
			.filter((s) => s.distanceKm <= area.radiusKm)
			.sort((a, b) => a.distanceKm - b.distanceKm)
			.slice(0, DEFAULT_AREA.maxPerChain)
	);
	return { radiusKm: area.radiusKm, stores: [...area.stores, ...extra] };
}
