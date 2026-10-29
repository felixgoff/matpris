// Coop Sweden exposes the APIs its own web shop calls via Azure APIM:
//   POST https://external.api.coop.se/personalization/search/global?api-version=v2&…
//   GET  https://proxy.api.coop.se/external/store/stores/map?conceptIds=12,6,95&…&api-version=v2
// Both take an Ocp-Apim-Subscription-Key, and coop.se hands those keys to every visitor
// in its page source (they ship in the site's own config JSON). We read them from there,
// like a browser does, and cache them for a day. Like the other chains' online shops the
// prices are one national list (store=251300), so they are treated as chain-wide.
import type { ChainId, ProductHit, Store } from '../../types';
import { TtlCache } from '../cache';
import { parseSek, toUnit } from '../normalize';
import type { ChainAdapter } from './types';

const SEARCH = 'https://external.api.coop.se/personalization/search/global';
const STORES = 'https://proxy.api.coop.se/external/store/stores/map';
const PAGE = 'https://www.coop.se/'; // page source carries the subscription keys

// The site spaces its own API traffic loosely; match that.
const INTERVAL_MS = 2_000;

// Shapes verified against live responses (2026-10-29).
export interface CoopPromotion {
	type: string;
	/** Total bundle price for MULTI_BUY promos, item price for FIXED_PRICE. */
	priceData?: { b2cPrice?: number };
	/** How many items a MULTI_BUY price covers. */
	numberOfProductRequired?: number;
	/** Coop MedMera member price: needs a logged-in account, skipped like City Gross's. */
	medMeraRequired?: boolean;
}

export interface CoopProduct {
	id: string;
	name: string;
	manufacturerName?: string | null;
	packageSizeInformation?: string | null;
	salesPriceData?: { b2cPrice?: number };
	comparativePriceData?: { b2cPrice?: number };
	comparativePriceUnit?: { unit?: string; text?: string };
	onlinePromotions?: CoopPromotion[];
	imageUrl?: string | null;
	availableOnline?: boolean;
}

interface CoopStore {
	storeId: number;
	name: string;
	address: string;
	postalCode?: string;
	city?: string;
	latitude: number;
	longitude: number;
	url?: string;
}

/** Minimum time between Coop API calls, shared across the adapter's requests. */
const nextSlot = { at: 0 };
async function throttled<T>(run: () => Promise<T>): Promise<T> {
	const wait = Math.max(0, nextSlot.at - Date.now());
	nextSlot.at = Date.now() + wait + INTERVAL_MS;
	if (wait) await new Promise((r) => setTimeout(r, wait));
	return run();
}

// The keys are printed in coop.se's own page config, refreshed rarely.
const keys = new TtlCache<{ personalization: string; store: string }>(24 * 60 * 60 * 1000);

async function subscriptionKeys(): Promise<{ personalization: string; store: string }> {
	return keys.get('keys', async () => {
		const res = await fetch(PAGE, { headers: { accept: 'text/html' }, signal: AbortSignal.timeout(15_000) });
		if (!res.ok) throw new Error(`coop.se answered ${res.status}`);
		const html = await res.text();
		const get = (name: string) => html.match(new RegExp(`"${name}"\\s*:\\s*"([0-9a-fA-F]{32})"`))?.[1];
		const personalization = get('personalizationApiSubscriptionKey');
		const store = get('storeApiSubscriptionKey');
		if (!personalization || !store) throw new Error('no Coop subscription keys in page source');
		return { personalization, store };
	});
}

export function toHit(p: CoopProduct, chain: ChainId = 'coop'): ProductHit | undefined {
	const price = p.salesPriceData?.b2cPrice;
	if (!price) return undefined;
	const unit = toUnit(p.comparativePriceUnit?.unit);
	const unitPrice = p.comparativePriceData?.b2cPrice;
	const hit: ProductHit = {
		chain,
		id: p.id,
		name: p.name,
		brand: p.manufacturerName ?? undefined,
		size: p.packageSizeInformation ?? undefined,
		price,
		unitPrice: unit && unitPrice ? unitPrice : undefined,
		unit: unit && unitPrice ? unit : undefined,
		// Coop's names are self-descriptive ("Fanta Orange"), so — like the Axfood chains —
		// no category path goes to the relevance tiers: its shelf names ("Läsk") would
		// wrongly read the query word as a flavour of another product.
		// cloudinary TIFF originals: request what the site itself serves (webp, compressed)
		image: p.imageUrl?.includes('/upload/')
			? `https://res.cloudinary.com/coopsverige/image/upload/f_auto,q_80/${p.imageUrl.split('/upload/')[1]}`
			: undefined,
		url: `https://www.coop.se/handla/varor/${slug(p.name)}-${p.id}`,
		pricing: 'chain'
	};
	// Deals: skip MedMera member prices (like City Gross), take the cheapest remaining.
	let best = hit;
	for (const promo of p.onlinePromotions ?? []) {
		const promoPrice = promo.medMeraRequired ? undefined : promo.priceData?.b2cPrice;
		if (!promoPrice) continue;
		const quantity = promo.type === 'MULTI_BUY_FIXED_PRICE' ? promo.numberOfProductRequired ?? 1 : 1;
		if (quantity > 1) {
			const each = Math.round((promoPrice / quantity) * 100) / 100;
			if (each >= best.price) continue;
			best = { ...best, deal: { quantity, each, total: promoPrice, mixAndMatch: false, plusDeposit: false } };
		} else if (promoPrice < best.price) {
			const par = hit.unitPrice !== undefined && best.unit && best.unitPrice !== undefined ? (best.unitPrice * promoPrice) / hit.price : undefined;
			best = {
				...best,
				price: promoPrice,
				oldPrice: best.price,
				unitPrice: par !== undefined ? Math.round(par * 100) / 100 : undefined,
				unit: par !== undefined ? best.unit : undefined
			};
		}
	}
	return best;
}

const slug = (name: string) =>
	name
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/(^-|-$)/g, '');

const storeList = new TtlCache<CoopStore[]>(24 * 60 * 60 * 1000);

export const coop: ChainAdapter = {
	chain: 'coop',
	pricing: 'chain',
	async listStores() {
		const { store } = await subscriptionKeys();
		const raw = await storeList.get('all', () =>
			throttled(async () => {
				const res = await fetch(`${STORES}?conceptIds=12,6,95&invertFilter=true&api-version=v2`, {
					headers: { 'ocp-apim-subscription-key': store, accept: 'application/json' },
					signal: AbortSignal.timeout(15_000)
				});
				if (!res.ok) throw new Error(`proxy.api.coop.se answered ${res.status}`);
				return (await res.json()) as CoopStore[];
			})
		);
		return raw
			.filter((s) => s.latitude && s.name)
			.map(
				(s): Store => ({
					id: String(s.storeId),
					chain: 'coop',
					name: s.name,
					lat: s.latitude,
					lng: s.longitude,
					address: [s.address, [s.postalCode, s.city].filter(Boolean).join(' ')].filter(Boolean).join(', '),
					url: s.url ? `https://www.coop.se${s.url}` : undefined
				})
			);
	},
	async search(query) {
		const { personalization } = await subscriptionKeys();
		const data = await throttled(() =>
			postJson<GlobalSearchResponse>(localize(SEARCH), personalization, {
				query,
				resultsOptions: { skip: 0, take: 40, sortBy: [], facets: [] },
				storesResultsOptions: { skip: 0, take: 1, sortBy: [], facets: [] }
			})
		);
		return (data.results?.items ?? [])
			.filter((p) => p.availableOnline !== false)
			.map((p) => toHit(p))
			.filter((h): h is ProductHit => !!h && h.price > 0);
	}
};

const localize = (url: string) =>
	`${url}?api-version=v2&store=251300&groups=CUSTOMER_PRIVATE&device=desktop&direct=false`;

function postJson<T>(url: string, key: string, body: unknown): Promise<T> {
	return throttled(async () => {
		const res = await fetch(url, {
			method: 'POST',
			headers: { 'ocp-apim-subscription-key': key, origin: 'https://www.coop.se', 'content-type': 'application/json' },
			body: JSON.stringify(body),
			signal: AbortSignal.timeout(8_000)
		});
		if (!res.ok) throw new Error(`external.api.coop.se answered ${res.status}`);
		return (await res.json()) as T;
	});
}

interface GlobalSearchResponse {
	results?: { count?: number; items?: CoopProduct[] };
}
