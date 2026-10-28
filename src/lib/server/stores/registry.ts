import type { Store } from '../../types';
import { TtlCache } from '../cache';
import { adapters } from '../chains';

const DAY = 24 * 60 * 60 * 1000;
const cache = new TtlCache<Store[]>(DAY);

/** Every known store of every chain. A chain whose store list fails is skipped. */
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
