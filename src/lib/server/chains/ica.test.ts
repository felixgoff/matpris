import { describe, expect, it } from 'vitest';
import { toHit, type IcaIndexEntry } from './ica';

// Shapes recorded by scripts/ica-catalogue.mjs from the shop's own API responses.
const base: IcaIndexEntry = {
	id: '10866423',
	name: 'Fanta Exotic Läsk 1,5 l PET',
	brand: 'Fanta',
	price: 19.9,
	unitPrice: 13.27,
	unit: 'kr/l',
	size: '1,5l',
	available: true,
	url: 'https://handlaprivatkund.ica.se/stores/1088004/products/fanta-exotic-lask-1-5l-pet/10866423'
};

describe('ICA toHit', () => {
	it('maps a priced product as chain pricing', () => {
		const hit = toHit(base);
		expect(hit).toMatchObject({
			chain: 'ica',
			id: '10866423',
			name: 'Fanta Exotic Läsk 1,5 l PET',
			price: 19.9,
			unitPrice: 13.27,
			unit: 'l',
			pricing: 'chain'
		});
		expect(hit.deal).toBeUndefined();
		expect(hit.oldPrice).toBeUndefined();
	});

	it('keeps a bare comparison unit ("kr/kg" already split)', () => {
		expect(
			toHit({ ...base, unit: 'kg' })
		).toMatchObject({ unitPrice: 13.27, unit: 'kg' });
	});

	it('derives units from the printed "kr/kg" and the size text when absent', () => {
		expect(toHit({ ...base, unit: undefined })).toMatchObject({ unitPrice: 13.27, unit: 'l' });
		expect(
			toHit({ ...base, unit: undefined, unitPrice: undefined, size: '500 g', price: 29.5 })
		).toMatchObject({ unitPrice: 59, unit: 'kg' });
	});
});
