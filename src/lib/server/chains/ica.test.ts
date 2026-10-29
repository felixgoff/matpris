import { describe, expect, it } from 'vitest';
import { toHit, type IcaProduct } from './ica';

// Shapes captured verbatim from the shop's own search + product APIs (2026-10-29).
const fanta: IcaProduct = {
	productId: 'abc2c8a2-9f59-4c77-9e8f-c8ea08cee898',
	retailerProductId: '2294810',
	type: 'REGULAR',
	name: 'Läsk Orange 1,5l Fanta',
	brand: 'Fanta',
	packSizeDescription: '1.5L',
	price: { amount: '19.40', currency: 'SEK' },
	unitPrice: { price: { amount: '12.93', currency: 'SEK' }, unitName: 'PER_LITRE_WITHOUT_DEPOSIT' },
	depositPrice: { amount: '3.00', currency: 'SEK' },
	available: true
};

const sale: IcaProduct = {
	...fanta,
	productId: 'c5e3ebc4-2054-42ca-a15a-e67d648703d9',
	retailerProductId: '1237504',
	name: 'Satsumas ca 100g Klass 1 ICA',
	packSizeDescription: '96g - 1000000g',
	price: { amount: '2.99', currency: 'SEK' },
	unitPrice: { price: { amount: '29.90', currency: 'SEK' }, unitName: 'PER_1KG' },
	promoPrice: { amount: '1.50', currency: 'SEK' },
	promoUnitPrice: { price: { amount: '15.00', currency: 'SEK' }, unitName: 'PER_1KG' }
};

describe('ICA toHit', () => {
	it('maps a regular product', () => {
		const hit = toHit(fanta);
		expect(hit).toMatchObject({
			chain: 'ica',
			price: 19.4,
			unitPrice: 12.93,
			unit: 'l',
			brand: 'Fanta',
			pricing: 'chain'
		});
		expect(hit.oldPrice).toBeUndefined();
		expect(hit.deal).toBeUndefined();
		expect(hit.url).toContain('/products/läsk-orange-1-5l-fanta/2294810');
	});

	it('maps a promotion price as the current price, with the regular price as oldPrice', () => {
		const hit = toHit(sale);
		expect(hit).toMatchObject({ price: 1.5, oldPrice: 2.99, unitPrice: 15, unit: 'kg' });
	});

	it('keeps a full-price promo as the regular price', () => {
		const hit = toHit({ ...fanta, promoPrice: { amount: '19.40' } });
		expect(hit).toMatchObject({ price: 19.4, unitPrice: 12.93 });
		expect(hit.oldPrice).toBeUndefined();
	});

	it("reads the piece unit ('EACH') for non-weighed products", () => {
		expect(
			toHit({
				...fanta,
				unitPrice: { price: { amount: '2.03' }, unitName: 'EACH' }
			}).unit
		).toBe('st');
	});
});
