// Willys and Hemköp both run on Axfood's platform and expose the JSON their own
// web shops use:
//   GET https://www.<site>/search?q=<query>&size=<n>     product search
//   GET https://www.<site>/axfood/rest/store?online=false all stores incl. coordinates
// Search returns the price list of the site's default store; there is no
// unauthenticated way to pick another store, so prices are treated as chain-wide.
// robots.txt asks for Crawl-delay: 10, which `fetchJson` enforces per host.
import type { ChainId, ProductHit, Store } from '../../types';
import { fetchJson } from '../http';
import { parseSek, toUnit } from '../normalize';
import type { ChainAdapter } from './types';

interface AxfoodPromotion {
	qualifyingCount: number | null;
	conditionLabel: string | null;
	rewardLabel: string | null;
	price: { value: number } | null;
}

interface AxfoodProduct {
	code: string;
	name: string;
	manufacturer: string | null;
	displayVolume: string | null;
	priceValue: number;
	comparePrice: string | null;
	comparePriceUnit: string | null;
	outOfStock: boolean;
	image: { url: string } | null;
	potentialPromotions: AxfoodPromotion[];
}

interface AxfoodStore {
	storeId: string;
	name: string;
	geoPoint: { latitude: number; longitude: number };
	address: { formattedAddress: string | null };
}

const CRAWL_DELAY_MS = 10_000;

function toHit(chain: ChainId, base: string, p: AxfoodProduct): ProductHit {
	const promo = p.potentialPromotions[0];
	const singleItemDeal = promo?.qualifyingCount === 1 && promo.price && promo.price.value < p.priceValue;
	const price = singleItemDeal ? promo.price!.value : p.priceValue;
	let unitPrice = parseSek(p.comparePrice);
	// comparePrice reflects the regular price; scale it to the deal price.
	if (unitPrice !== undefined && singleItemDeal) unitPrice = Math.round((unitPrice * price * 100) / p.priceValue) / 100;

	return {
		chain,
		id: p.code,
		name: p.name,
		brand: p.manufacturer ?? undefined,
		size: p.displayVolume ?? undefined,
		price,
		unitPrice,
		unit: toUnit(p.comparePriceUnit),
		oldPrice: singleItemDeal ? p.priceValue : undefined,
		// A single-item deal is already reflected in price/oldPrice; only multi-buys need their label.
		promo: singleItemDeal
			? promo.conditionLabel || 'Kampanjpris'
			: promo
				? [promo.rewardLabel, promo.conditionLabel].filter(Boolean).join(' · ') || undefined
				: undefined,
		image: p.image?.url,
		url: `${base}/sok?q=${encodeURIComponent(p.name)}`,
		pricing: 'chain'
	};
}

function axfoodAdapter(chain: ChainId, base: string): ChainAdapter {
	return {
		chain,
		pricing: 'chain',
		async listStores() {
			const stores = await fetchJson<AxfoodStore[]>(`${base}/axfood/rest/store?online=false`, {
				intervalMs: CRAWL_DELAY_MS,
				maxWaitMs: 30_000,
				timeoutMs: 20_000
			});
			return stores
				.filter((s) => s.geoPoint?.latitude && s.name)
				.map(
					(s): Store => ({
						id: s.storeId,
						chain,
						name: s.name,
						lat: s.geoPoint.latitude,
						lng: s.geoPoint.longitude,
						address: s.address?.formattedAddress ?? ''
					})
				);
		},
		async search(query) {
			const data = await fetchJson<{ results: AxfoodProduct[] }>(
				`${base}/search?q=${encodeURIComponent(query)}&size=40`,
				{ intervalMs: CRAWL_DELAY_MS }
			);
			return data.results.filter((p) => !p.outOfStock && p.priceValue > 0).map((p) => toHit(chain, base, p));
		}
	};
}

export const willys = axfoodAdapter('willys', 'https://www.willys.se');
export const hemkop = axfoodAdapter('hemkop', 'https://www.hemkop.se');
