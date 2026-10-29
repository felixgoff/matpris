// Types shared between server and browser.

export type ChainId = 'willys' | 'hemkop' | 'lidl';

// Logos are the chains' own SVGs from their websites, served from static/logos.
export const CHAINS: Record<ChainId, { name: string; color: string; logo: string }> = {
	willys: { name: 'Willys', color: '#e30613', logo: '/logos/willys.svg' },
	hemkop: { name: 'Hemköp', color: '#f07800', logo: '/logos/hemkop.svg' },
	lidl: { name: 'Lidl', color: '#0050aa', logo: '/logos/lidl.svg' }
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

export interface ProductHit {
	chain: ChainId;
	id: string;
	name: string;
	brand?: string;
	size?: string;
	/** Price you pay for one item right now (campaign price if a single-item campaign applies). */
	price: number;
	/** Comparison price ("jämförpris") in kr per unit. */
	unitPrice?: number;
	unit?: Unit;
	/** Regular price when `price` is a campaign price. */
	oldPrice?: number;
	/** Human-readable deal text, e.g. "3 för 50 kr". */
	promo?: string;
	image?: string;
	url: string;
	/** Where the price applies: one store, or the whole chain. */
	pricing: 'store' | 'chain';
}

export interface ResultRow extends ProductHit {
	/** Nearest store in the search circle where this price applies. */
	store: NearbyStore;
	/** 'weak' = the query only starts a longer word ("fanta" in "Fantasty"); hidden when strong matches exist. */
	match: 'strong' | 'weak';
}

export type ChainStatus = 'ok' | 'error' | 'throttled' | 'no-stores';

/** One line of the NDJSON stream returned by /api/search. */
export type SearchEvent =
	| { type: 'area'; center: { lat: number; lng: number }; radiusKm: number; stores: NearbyStore[] }
	| { type: 'chain'; chain: ChainId; status: ChainStatus; results: ResultRow[]; message?: string }
	| { type: 'done' };
