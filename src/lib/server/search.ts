import type { ChainId, NearbyStore, ProductHit, ResultRow, SearchEvent } from '../types';
import { TtlCache } from './cache';
import { adapters } from './chains';
import type { ChainAdapter } from './chains/types';
import { ThrottledError } from './http';
import { relevance } from './normalize';
import { MAP_ONLY_CHAINS, pricedArea, withMapOnlyStores } from './stores/registry';

const priceCache = new TtlCache<ProductHit[]>(15 * 60 * 1000);

export function normalizeQuery(q: string) {
	return q.trim().toLowerCase().replace(/\s+/g, ' ').slice(0, 60);
}

async function searchChain(adapter: ChainAdapter, query: string, stores: NearbyStore[]): Promise<ResultRow[]> {
	const chain = adapter.chain;
	const rate = (hits: ProductHit[], store: NearbyStore): ResultRow[] =>
		hits.flatMap((h) => {
			const match = relevance(query, h.name, h.brand, h.categories);
			return match ? [{ ...h, store, match }] : [];
		});

	if (adapter.pricing === 'chain') {
		// One national price list: price it once, point at the nearest store.
		const hits = await priceCache.get(`${chain}:${query}`, () => adapter.search(query));
		return rate(hits, stores[0]);
	}

	// Per-store prices: query every store in the circle, keep each product's best offer.
	const perStore = await Promise.allSettled(
		stores.map((s) => priceCache.get(`${chain}:${s.id}:${query}`, () => adapter.search(query, s)))
	);
	const best = new Map<string, ResultRow>();
	perStore.forEach((r, i) => {
		if (r.status !== 'fulfilled') return;
		for (const row of rate(r.value, stores[i])) {
			const prev = best.get(row.id);
			if (!prev || row.price < prev.price) best.set(row.id, row);
		}
	});
	if (!best.size && perStore.every((r) => r.status === 'rejected')) throw (perStore[0] as PromiseRejectedResult).reason;
	return [...best.values()];
}

/**
 * Yields the search area first, then each chain's results as soon as they arrive.
 * Stores of chains without online prices (ICA, Coop) are looked up in parallel and
 * sent as a second, complete `area` event, so they never delay the prices.
 */
export async function* search(
	rawQuery: string,
	center: { lat: number; lng: number },
	pickedKm?: number
): AsyncGenerator<SearchEvent> {
	const query = normalizeQuery(rawQuery);
	const area = await pricedArea(center, pickedKm);
	yield { type: 'area', center, ...area };

	const byChain = new Map<ChainId, NearbyStore[]>();
	for (const s of area.stores) byChain.set(s.chain, [...(byChain.get(s.chain) ?? []), s]);

	const pending = new Map<string, Promise<SearchEvent[]>>();
	for (const adapter of Object.values(adapters)) {
		const chain = adapter.chain;
		const stores = byChain.get(chain);
		pending.set(
			chain,
			!stores
				? Promise.resolve([{ type: 'chain', chain, status: 'no-stores', results: [] }])
				: searchChain(adapter, query, stores).then(
						(results): SearchEvent[] => [{ type: 'chain', chain, status: 'ok', results }],
						(e: Error): SearchEvent[] => [
							{
								type: 'chain',
								chain,
								status: e instanceof ThrottledError ? 'throttled' : 'error',
								results: [],
								message: e.message
							}
						]
					)
		);
	}
	pending.set(
		'map-only',
		withMapOnlyStores(center, area).then((full): SearchEvent[] => [
			{ type: 'area', center, ...full },
			...MAP_ONLY_CHAINS.map((chain): SearchEvent => ({ type: 'chain', chain, status: 'no-online-prices', results: [] }))
		])
	);

	const keyed = (key: string) => pending.get(key)!.then((events) => ({ key, events }));
	while (pending.size) {
		const { key, events } = await Promise.race([...pending.keys()].map(keyed));
		pending.delete(key);
		yield* events;
	}
	yield { type: 'done' };
}
