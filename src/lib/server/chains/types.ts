import type { ChainId, ProductHit, Store } from '../../types';

export interface ChainAdapter {
	chain: ChainId;
	/**
	 * 'chain': one price list for the whole chain, so one search covers every store.
	 * 'store': prices differ per store, so `search` is called once per store in the circle.
	 */
	pricing: 'chain' | 'store';
	listStores(): Promise<Store[]>;
	search(query: string, store?: Store): Promise<ProductHit[]>;
}
