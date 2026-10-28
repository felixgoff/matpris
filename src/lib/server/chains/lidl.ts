// Lidl Sweden has no full online grocery shop; its site search covers the
// current assortment and weekly deals, with one national price list:
//   GET https://www.lidl.se/q/api/search?q=<query>&assortment=SE&locale=sv_SE&version=v2.0.0&fetchsize=<n>
// Store locations come from data/lidl-stores.json (see scripts/build-lidl-stores.mjs).
import type { ProductHit, Store } from '../../types';
import lidlStores from '../data/lidl-stores.json';
import { fetchJson } from '../http';
import { parseUnitPrice, unitPriceFromSize } from '../normalize';
import type { ChainAdapter } from './types';

interface LidlItem {
	code: string;
	gridbox?: {
		data: {
			fullTitle: string;
			brand?: { name?: string } | string;
			category?: string;
			canonicalUrl?: string;
			image?: string;
			price?: {
				price?: number;
				oldPrice?: number;
				basePrice?: { text?: string };
				packaging?: { text?: string };
			};
		};
	};
}

const FOOD_CATEGORIES = new Set(['Food', 'F+V']);

export const lidl: ChainAdapter = {
	chain: 'lidl',
	pricing: 'chain',
	async listStores() {
		return lidlStores as Store[];
	},
	async search(query) {
		const data = await fetchJson<{ items: LidlItem[] }>(
			`https://www.lidl.se/q/api/search?q=${encodeURIComponent(query)}&assortment=SE&locale=sv_SE&version=v2.0.0&fetchsize=40`,
			{ intervalMs: 1000 }
		);
		const hits: ProductHit[] = [];
		for (const item of data.items) {
			const d = item.gridbox?.data;
			const price = d?.price?.price;
			if (!d || !price || !FOOD_CATEGORIES.has(d.category ?? '')) continue;
			const size = d.price?.packaging?.text;
			const unit = parseUnitPrice(d.price?.basePrice?.text) ?? unitPriceFromSize(price, size);
			const brand = typeof d.brand === 'string' ? d.brand : d.brand?.name;
			hits.push({
				chain: 'lidl',
				id: item.code,
				// Lidl titles often start with the brand in caps ("MATRIKET Standardmjölk").
				name: brand && d.fullTitle.startsWith(brand) ? d.fullTitle.slice(brand.length).trim() : d.fullTitle,
				brand,
				size,
				price,
				unitPrice: unit?.unitPrice,
				unit: unit?.unit,
				oldPrice: d.price?.oldPrice && d.price.oldPrice > price ? d.price.oldPrice : undefined,
				image: d.image,
				url: `https://www.lidl.se${d.canonicalUrl ?? ''}`,
				pricing: 'chain'
			});
		}
		return hits;
	}
};
