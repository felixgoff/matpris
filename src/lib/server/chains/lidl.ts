// Lidl Sweden has no full online grocery shop; its site search covers the
// current assortment and weekly deals, with one national price list:
//   GET https://www.lidl.se/q/api/search?q=<query>&assortment=SE&locale=sv_SE&version=v2.0.0&fetchsize=<n>
// Store locations come from data/lidl-stores.json (see scripts/build-lidl-stores.mjs).
import type { ProductHit, Store } from '../../types';
import lidlStores from '../data/lidl-stores.json';
import { fetchJson } from '../http';
import { parseUnitPrice, unitPriceFromSize } from '../normalize';
import type { ChainAdapter } from './types';

interface LidlPrice {
	price?: number;
	oldPrice?: number;
	basePrice?: { text?: string };
	packaging?: { text?: string };
}

export interface LidlItem {
	code: string;
	gridbox?: {
		data: {
			fullTitle: string;
			brand?: { name?: string } | string;
			category?: string;
			canonicalUrl?: string;
			image?: string;
			price?: LidlPrice;
			regionsV2?: Record<string, { isDefault?: boolean; regionPriceId?: string }>;
			/** Per price region; an item on a Lidl Plus offer has only `currentLidlPlusPrice`. */
			regionsPrices?: Record<string, { currentLidlPlusPrice?: { price?: LidlPrice } }>;
		};
	};
}

const FOOD_CATEGORIES = new Set(['Food', 'F+V']);

export function toHit(item: LidlItem): ProductHit | undefined {
	const d = item.gridbox?.data;
	if (!d || !FOOD_CATEGORIES.has(d.category ?? '')) return undefined;
	// An item on a Lidl Plus offer has no plain price: the offer's `price` is the member
	// price and its `oldPrice` (struck through on the site) is what everyone else pays.
	// Member prices are skipped, like City Gross's and Coop's.
	const region = Object.values(d.regionsV2 ?? {}).find((r) => r.isDefault)?.regionPriceId ?? '1';
	const plus = d.price?.price ? undefined : d.regionsPrices?.[region]?.currentLidlPlusPrice?.price;
	const price = d.price?.price ?? plus?.oldPrice;
	if (!price) return undefined;
	const size = d.price?.packaging?.text ?? plus?.packaging?.text;
	const base = (plus ?? d.price)?.basePrice?.text;
	// A bare "/kg" base price means the price itself is per kilo (loose fruit). A Lidl Plus
	// offer's kr/l or kr/kg is the member's, so the unit price comes from the size instead.
	const perKg = /^\s*\/\s*kg\b/i.test(base ?? '');
	const unit = perKg
		? { unitPrice: price, unit: 'kg' as const }
		: ((plus ? undefined : parseUnitPrice(base)) ?? unitPriceFromSize(price, size));
	const brand = typeof d.brand === 'string' ? d.brand : d.brand?.name;
	return {
		chain: 'lidl',
		id: item.code,
		// Lidl titles often start with the brand in caps ("MATRIKET Standardmjölk").
		name: brand && d.fullTitle.startsWith(brand) ? d.fullTitle.slice(brand.length).trim() : d.fullTitle,
		brand,
		size,
		price,
		unitPrice: unit?.unitPrice,
		unit: unit?.unit,
		perKg: perKg || undefined,
		oldPrice: d.price?.oldPrice && d.price.oldPrice > price ? d.price.oldPrice : undefined,
		image: d.image,
		url: `https://www.lidl.se${d.canonicalUrl ?? ''}`,
		pricing: 'chain'
	};
}

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
		return data.items.flatMap((item) => toHit(item) ?? []);
	}
};
