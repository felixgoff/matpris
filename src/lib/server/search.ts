import type { ChainId, NearbyStore, ProductHit, ResultRow, SearchEvent } from '../types';
import { TtlCache } from './cache';
import { adapters } from './chains';
import type { ChainAdapter } from './chains/types';
import { ThrottledError } from './http';
import { matchesQuery } from './normalize';
import { pickArea } from './stores/radius';
import { allStores } from './stores/registry';

const priceCache = new TtlCache<ProductHit[]>(15 * 60 * 1000);

export function normalizeQuery(q: string) {
	return q.trim().toLowerCase().replace(/\s+/g, ' ').slice(0, 60);
}

async function searchChain(adapter: ChainAdapter, query: string, stores: NearbyStore[]): Promise<ResultRow[]> {
	const chain = adapter.chain;
	const relevant = (h: ProductHit) => matchesQuery(query, h.name, h.brand);

	if (adapter.pricing === 'chain') {
		// One national price list: price it once, point at the nearest store.
		const hits = await priceCache.get(`${chain}:${query}`, () => adapter.search(query));
		return hits.filter(relevant).map((h) => ({ ...h, store: stores[0] }));
	}

	// Per-store prices: query every store in the circle, keep each product's best offer.
	const perStore = await Promise.allSettled(
		stores.map((s) => priceCache.get(`${chain}:${s.id}:${query}`, () => adapter.search(query, s)))
	);
	const best = new Map<string, ResultRow>();
	perStore.forEach((r, i) => {
		if (r.status !== 'fulfilled') return;
		for (const h of r.value.filter(relevant)) {
			const prev = best.get(h.id);
			if (!prev || h.price < prev.price) best.set(h.id, { ...h, store: stores[i] });
		}
	});
	if (!best.size && perStore.every((r) => r.status === 'rejected')) throw (perStore[0] as PromiseRejectedResult).reason;
	return [...best.values()];
}

/** Yields the search area first, then each chain's results as soon as they arrive. */
export async function* search(rawQuery: string, center: { lat: number; lng: number }): AsyncGenerator<SearchEvent> {
	const query = normalizeQuery(rawQuery);
	const area = pickArea(center, await allStores());
	yield { type: 'area', center, ...area };

	const byChain = new Map<ChainId, NearbyStore[]>();
	for (const s of area.stores) byChain.set(s.chain, [...(byChain.get(s.chain) ?? []), s]);

	const pending = new Map<ChainId, Promise<SearchEvent & { type: 'chain' }>>();
	for (const adapter of Object.values(adapters)) {
		const chain = adapter.chain;
		const stores = byChain.get(chain);
		pending.set(
			chain,
			!stores
				? Promise.resolve({ type: 'chain', chain, status: 'no-stores', results: [] })
				: searchChain(adapter, query, stores).then(
						(results) => ({ type: 'chain', chain, status: 'ok', results }),
						(e: Error) => ({
							type: 'chain',
							chain,
							status: e instanceof ThrottledError ? 'throttled' : 'error',
							results: [],
							message: e.message
						})
					)
		);
	}

	while (pending.size) {
		const event = await Promise.race(pending.values());
		pending.delete(event.chain);
		yield event;
	}
	yield { type: 'done' };
}
