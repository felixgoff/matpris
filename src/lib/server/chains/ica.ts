// ICA's shop (handlaprivatkund.ica.se) sits behind an AWS WAF that challenges
// automated requests, so there is nothing sane to call at search time (see README).
// Instead prices come from a locally built index, data/ica-catalogue.json, assembled
// by scripts/ica-catalogue.mjs: a real Chrome window browses one store like a
// shopper and records what it sees. The index is one store's priced assortment
// (default Maxi ICA Gunnesbo, Lund), presented like Lidl's list: chain pricing,
// a good approximation for the nearby stores ranked alongside it. The dev server
// needs a restart to see a newly rebuilt index.
import type { ProductHit, Store, Unit } from '../../types';
import icaStores from '../data/ica-stores.json';
import icaCatalogue from '../data/ica-catalogue.json';
import { matchQuality, parseUnitPrice, toUnit, unitPriceFromSize } from '../normalize';
import type { ChainAdapter } from './types';

export interface IcaIndexEntry {
	id: string;
	name: string;
	brand?: string;
	price: number;
	/** Comparison price ("jämförpris") in kr per unit. */
	unitPrice?: number;
	/** Comparison unit as recorded from the site: "kr/kg", "kr/l", "kr/st" or a bare unit. */
	unit?: string;
	size?: string;
	available?: boolean;
	ean?: string;
	categories?: string[];
	url: string;
}

const products = ((icaCatalogue as { products?: IcaIndexEntry[] }).products ?? []) as IcaIndexEntry[];

/** "kr/kg" → { parsed: comparison price }; a bare unit ("kg") just names the unit. */
const printedUnit = (unit: string | undefined) => ({ bare: toUnit(unit), parsed: parseUnitPrice(unit) });

export function toHit(entry: IcaIndexEntry): ProductHit {
	const { bare, parsed } = printedUnit(entry.unit);
	const derived = unitPriceFromSize(entry.price, entry.size);
	return {
		chain: 'ica',
		id: entry.id,
		name: entry.name,
		brand: entry.brand,
		size: entry.size,
		price: entry.price,
		unitPrice: entry.unitPrice ?? parsed?.unitPrice ?? derived?.unitPrice,
		unit: (bare ?? parsed?.unit ?? derived?.unit) as Unit | undefined,
		url: entry.url,
		pricing: 'chain'
	};
}

export const ica: ChainAdapter = {
	chain: 'ica',
	pricing: 'chain',
	async listStores() {
		return icaStores as Store[];
	},
	async search(query) {
		const hits: ProductHit[] = [];
		for (const entry of products) {
			if (entry.available === false) continue;
			if (!matchQuality(query, entry.name, entry.brand)) continue;
			hits.push(toHit(entry));
		}
		return hits;
	}
};
