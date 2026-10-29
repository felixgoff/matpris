// Types shared between server and browser.

export type ChainId = 'willys' | 'hemkop' | 'lidl' | 'ica' | 'coop';

// Logos are served from static/logos (see README for sources).
// `online: false` chains have no public price data: their stores are shown, their prices aren't.
export const CHAINS: Record<ChainId, { name: string; color: string; logo: string; online: boolean }> = {
	willys: { name: 'Willys', color: '#e30613', logo: '/logos/willys.svg', online: true },
	hemkop: { name: 'Hemköp', color: '#f07800', logo: '/logos/hemkop.svg', online: true },
	lidl: { name: 'Lidl', color: '#0050aa', logo: '/logos/lidl.svg', online: true },
	ica: { name: 'ICA', color: '#e3000b', logo: '/logos/ica.svg', online: false },
	coop: { name: 'Coop', color: '#00aa46', logo: '/logos/coop.svg', online: false }
};

export interface Store {
	id: string;
	chain: ChainId;
	name: string;
	lat: number;
	lng: number;
	address: string;
	url?: string;
}

export interface NearbyStore extends Store {
	distanceKm: number;
}

export type Unit = 'kg' | 'l' | 'st';

/** A multi-buy offer such as "5 for 55 kr". */
export interface Deal {
	/** Items you must buy to get the deal price. */
	quantity: number;
	/** Price per item when buying `quantity`. */
	each: number;
	total: number;
	/** Comparison price per unit at the deal price. */
	unitPrice?: number;
	/** The quantity can be any mix of the products in the offer. */
	mixAndMatch: boolean;
	/** Bottle deposit ("pant") is charged on top. */
	plusDeposit: boolean;
	/** Maximum number of deal purchases per customer. */
	limit?: number;
}

export interface ProductHit {
	chain: ChainId;
	id: string;
	name: string;
	brand?: string;
	size?: string;
	/** Price you pay for one item right now (sale price if a single-item sale applies). */
	price: number;
	/** Comparison price ("jämförpris") in kr per unit. */
	unitPrice?: number;
	unit?: Unit;
	/** `price` is per kilo: the product is sold by weight (loose bananas, tomatoes). */
	perKg?: boolean;
	/** Estimated price of one piece of a product sold by weight, from its average weight. */
	approxEach?: number;
	/** Regular price when `price` is a sale price. */
	oldPrice?: number;
	deal?: Deal;
	image?: string;
	url: string;
	/** Where the price applies: one store, or the whole chain. */
	pricing: 'store' | 'chain';
}

export interface ResultRow extends ProductHit {
	/** Nearest store in the search circle where this price applies. */
	store: NearbyStore;
	/** How well it answers the query; see `relevance` in server/normalize.ts. */
	match: 'best' | 'mention' | 'weak';
}

export type ChainStatus = 'ok' | 'error' | 'throttled' | 'no-stores' | 'no-online-prices';

/** One line of the NDJSON stream returned by /api/search. */
export type SearchEvent =
	| { type: 'area'; center: { lat: number; lng: number }; radiusKm: number; stores: NearbyStore[] }
	| { type: 'chain'; chain: ChainId; status: ChainStatus; results: ResultRow[]; message?: string }
	| { type: 'done' };
