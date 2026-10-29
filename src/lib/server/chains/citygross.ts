// City Gross exposes the JSON its own web shop uses:
//   GET https://www.citygross.se/api/v1/Loop54/search?SearchQuery=<query>&type=product&Take=<n>
//   GET https://www.citygross.se/api/v1/PageData/stores   every store page, with coordinates
// Search returns the prices of the site's default store, so prices are treated as
// chain-wide (like Hemköp). robots.txt disallows the /loop54/ page route, not /api/v1/.
import type { ProductHit, Store, Unit } from '../../types';
import { fetchJson } from '../http';
import { parseSize } from '../normalize';
import type { ChainAdapter } from './types';

const BASE = 'https://www.citygross.se';
// No Crawl-delay in robots.txt; stay well below what a shopper's browser does.
const INTERVAL_MS = 2000;

interface CgPrice {
	price: number;
	/** "KGM" when the price is per kilo (sold by weight), "PCE" per item. */
	unit: string;
	comparativePrice?: number;
	/** "KGM", "LTR" or "PCE". */
	comparativePriceUnit?: string;
}

interface CgPromotion {
	effectType: string;
	membersOnly: boolean;
	minQuantity: number | null;
	/** Total price for `minQuantity` items. */
	value: number | null;
	maxAppliedPerReceipt: number;
	priceDetails: CgPrice | null;
}

export interface CgProduct {
	id: string;
	name: string;
	brand: string | null;
	descriptiveSize: string | null;
	superCategory?: string | null;
	category?: string | null;
	/** The most specific category, e.g. "Bryggkaffe", "Bananer". */
	bfCategory?: string | null;
	url: string;
	images: { url: string }[];
	productStoreDetails: {
		prices: { currentPrice: CgPrice; ordinaryPrice: CgPrice | null; activePromotion: CgPromotion | null } | null;
	} | null;
}

interface CgStorePage {
	data: {
		type: string;
		ispublished: boolean;
		siteId: number;
		storeName: string;
		url: string;
		storeLocation: { coordinates: string } | null;
		address: { streetAddress?: string; city?: string } | null;
	};
}

const UNITS: Record<string, Unit> = { KGM: 'kg', LTR: 'l', PCE: 'st' };

/** Brands come in capitals ("DAVE & JON'S"); sizes too ("CA 170G"). */
const titleCase = (s: string) => s.toLowerCase().replace(/(^|[\s&-])(\p{L})/gu, (m) => m.toUpperCase());

export function toHit(p: CgProduct): ProductHit | undefined {
	const prices = p.productStoreDetails?.prices;
	const current = prices?.currentPrice;
	if (!prices || !current?.price) return undefined;
	const perKg = current.unit === 'KGM';
	// A piece of loose fruit: its price per kilo times the approximate weight ("CA 170G").
	const weight = perKg ? parseSize(p.descriptiveSize)?.amount : undefined;
	const hit: ProductHit = {
		chain: 'citygross',
		id: p.id,
		name: p.name,
		brand: p.brand ? titleCase(p.brand) : undefined,
		size: p.descriptiveSize?.toLowerCase() ?? undefined,
		categories: [p.superCategory, p.category, p.bfCategory].filter((c): c is string => !!c),
		price: current.price,
		unitPrice: current.comparativePrice,
		unit: UNITS[current.comparativePriceUnit ?? ''],
		perKg: perKg || undefined,
		approxEach: weight ? Math.round(current.price * weight * 100) / 100 : undefined,
		image: p.images[0]?.url ? `${BASE}/images/products/${p.images[0].url}` : undefined,
		url: `${BASE}${p.url}`,
		pricing: 'chain'
	};
	const ordinary = prices.ordinaryPrice?.price;
	if (ordinary && ordinary > current.price) hit.oldPrice = ordinary;

	// Multi-buy offers ("2 för 22 kr"). Members-only prices need a City Gross account, so they're left out.
	const promo = prices.activePromotion;
	const each = promo?.priceDetails?.price;
	const quantity = promo?.minQuantity ?? 0;
	if (promo && !promo.membersOnly && promo.effectType === 'ItemsTotal' && quantity > 1 && each && each < current.price)
		hit.deal = {
			quantity,
			each,
			total: promo.value ?? Math.round(each * quantity * 100) / 100,
			unitPrice: promo.priceDetails?.comparativePrice,
			mixAndMatch: false,
			plusDeposit: false,
			limit: promo.maxAppliedPerReceipt > 0 ? promo.maxAppliedPerReceipt : undefined
		};
	return hit;
}

export const citygross: ChainAdapter = {
	chain: 'citygross',
	pricing: 'chain',
	async listStores() {
		const pages = await fetchJson<CgStorePage[]>(`${BASE}/api/v1/PageData/stores`, {
			intervalMs: INTERVAL_MS,
			timeoutMs: 20_000
		});
		return pages.flatMap(({ data: s }): Store[] => {
			const [lat, lng] = (s.storeLocation?.coordinates ?? '').split(',').map(Number);
			if (s.type !== 'StorePage' || !s.ispublished || !lat || !lng) return [];
			const street = s.address?.streetAddress?.trim();
			const city = s.address?.city?.trim();
			return [
				{
					id: `cg-${s.siteId}`,
					chain: 'citygross',
					name: `City Gross ${s.storeName.trim()}`,
					lat,
					lng,
					address: [street, city].filter(Boolean).join(', '),
					url: `${BASE}${s.url}`
				}
			];
		});
	},
	async search(query) {
		const data = await fetchJson<{ searchResults: { products: CgProduct[] } }>(
			`${BASE}/api/v1/Loop54/search?SearchQuery=${encodeURIComponent(query)}&type=product&Take=40`,
			{ intervalMs: INTERVAL_MS }
		);
		return data.searchResults.products.flatMap((p) => toHit(p) ?? []);
	}
};
