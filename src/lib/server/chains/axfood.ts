// Willys and Hemköp both run on Axfood's platform and expose the JSON their own
// web shops use:
//   GET https://www.<site>/search?q=<query>&size=<n>     product search
//   GET https://www.<site>/axfood/rest/store?online=false all stores incl. coordinates
// Search returns the price list of the site's default store; there is no
// unauthenticated way to pick another store, so prices are treated as chain-wide.
// robots.txt asks for Crawl-delay: 10, which `fetchJson` enforces per host. That one
// turn per 10 s goes to searches only: the store lists are prebuilt into data/ by
// scripts/build-axfood-stores.mjs (bun run stores:axfood).
import type { ChainId, ProductHit, Store } from '../../types';
import hemkopStores from '../data/hemkop-stores.json';
import willysStores from '../data/willys-stores.json';
import { fetchJson } from '../http';
import { parseSek, toUnit } from '../normalize';
import type { ChainAdapter } from './types';

interface AxfoodPromotion {
	qualifyingCount: number | null;
	rewardLabel: string | null;
	price: { value: number } | null;
	/** Comparison price at the promotion price, e.g. "7,33 kr". */
	comparePrice: string | null;
	promotionType: string | null;
	/** e.g. "Max 5 köp" */
	redeemLimitLabel: string | null;
}

export interface AxfoodProduct {
	code: string;
	name: string;
	manufacturer: string | null;
	displayVolume: string | null;
	priceValue: number;
	/** "kr/kg" when `priceValue` is per kilo (sold by weight), else "kr/st". */
	priceUnit?: string | null;
	/** Average weight of one piece in kg, for products sold by weight. */
	averageWeight?: number | null;
	comparePrice: string | null;
	comparePriceUnit: string | null;
	outOfStock: boolean;
	image: { url: string } | null;
	potentialPromotions: AxfoodPromotion[];
}

const CRAWL_DELAY_MS = 10_000;

export function toHit(chain: ChainId, base: string, p: AxfoodProduct): ProductHit {
	const promo = p.potentialPromotions[0];
	const promoPrice = promo?.price?.value;
	const regularUnitPrice = parseSek(p.comparePrice);
	// comparePrice reflects the regular price; scale it when the promotion has none.
	const scaled = (price: number) =>
		regularUnitPrice !== undefined ? Math.round((regularUnitPrice * price * 100) / p.priceValue) / 100 : undefined;
	const hit: ProductHit = {
		chain,
		id: p.code,
		name: p.name,
		brand: p.manufacturer ?? undefined,
		size: p.displayVolume ?? undefined,
		price: p.priceValue,
		unitPrice: regularUnitPrice,
		unit: toUnit(p.comparePriceUnit),
		image: p.image?.url,
		url: `${base}/sok?q=${encodeURIComponent(p.name)}`,
		pricing: 'chain'
	};
	if (p.priceUnit === 'kr/kg') {
		hit.perKg = true;
		if (p.averageWeight) hit.approxEach = Math.round(p.priceValue * p.averageWeight * 100) / 100;
	}
	if (!promo || promoPrice === undefined || promoPrice >= p.priceValue) return hit;

	const quantity = promo.qualifyingCount ?? 1;
	const unitPrice = parseSek(promo.comparePrice) ?? scaled(promoPrice);
	if (quantity <= 1) {
		// Single-item sale: it is simply the current price.
		const approxEach = p.averageWeight && hit.perKg ? Math.round(promoPrice * p.averageWeight * 100) / 100 : undefined;
		return { ...hit, price: promoPrice, oldPrice: p.priceValue, unitPrice, approxEach };
	}
	const limit = Number(promo.redeemLimitLabel?.match(/\d+/)?.[0]);
	hit.deal = {
		quantity,
		each: promoPrice,
		total: Math.round(promoPrice * quantity * 100) / 100,
		unitPrice,
		mixAndMatch: promo.promotionType === 'MixMatchPricePromotion',
		plusDeposit: /pant/i.test(promo.rewardLabel ?? ''),
		limit: limit > 0 ? limit : undefined
	};
	return hit;
}

function axfoodAdapter(chain: ChainId, base: string, stores: Store[]): ChainAdapter {
	return {
		chain,
		pricing: 'chain',
		async listStores() {
			return stores;
		},
		async search(query) {
			const data = await fetchJson<{ results: AxfoodProduct[] }>(
				`${base}/search?q=${encodeURIComponent(query)}&size=40`,
				// Wait for the next turn rather than fail; a newer search takes over a queued
				// turn, so the wait is never more than one Crawl-delay.
				{ intervalMs: CRAWL_DELAY_MS, maxWaitMs: 30_000, latestOnly: true }
			);
			return data.results.filter((p) => !p.outOfStock && p.priceValue > 0).map((p) => toHit(chain, base, p));
		}
	};
}

export const willys = axfoodAdapter('willys', 'https://www.willys.se', willysStores as Store[]);
export const hemkop = axfoodAdapter('hemkop', 'https://www.hemkop.se', hemkopStores as Store[]);
