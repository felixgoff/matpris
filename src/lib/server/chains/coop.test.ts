import { describe, expect, it } from 'vitest';
import { toHit, type CoopProduct } from './coop';

// Shapes taken from real external.api.coop.se responses (2026-10-29).
const base: CoopProduct = {
	id: '5000112667660',
	name: 'Fanta Orange 6-pack',
	manufacturerName: 'Fanta',
	packageSizeInformation: '6x330ml',
	salesPriceData: { b2cPrice: 45.38 },
	comparativePriceData: { b2cPrice: 22.92 },
	comparativePriceUnit: { unit: 'liter', text: 'kr/lit' },
	availableOnline: true
};

describe('Coop toHit', () => {
	it('maps a regular price', () => {
		const hit = toHit(base)!;
		expect(hit).toMatchObject({ price: 45.38, unitPrice: 22.92, unit: 'l' });
		// names are self-descriptive; no categories go to the relevance tiers
		expect(hit.categories).toBeUndefined();
		expect(hit.deal).toBeUndefined();
		expect(hit.oldPrice).toBeUndefined();
	});

	it('maps a multi-buy "2 för 29" deal', () => {
		const hit = toHit({
			...base,
			id: '5000112689792',
			name: 'Coca-Cola Zero 1,5l',
			salesPriceData: { b2cPrice: 22.9 },
			comparativePriceData: { b2cPrice: 15.27 },
			packageSizeInformation: '1,5l',
			onlinePromotions: [
				{
					type: 'MULTI_BUY_FIXED_PRICE',
					numberOfProductRequired: 2,
					priceData: { b2cPrice: 29 },
					medMeraRequired: false
				}
			]
		})!;
		expect(hit.price).toBe(22.9); // buying one still costs the regular price
		expect(hit.deal).toEqual({
			quantity: 2,
			each: 14.5,
			total: 29,
			mixAndMatch: false,
			plusDeposit: false
		});
	});

	it('maps a single-item sale as the current price', () => {
		const hit = toHit({
			...base,
			id: '345070',
			name: 'Chokladmjölk Pucko Original',
			packageSizeInformation: '1l',
			salesPriceData: { b2cPrice: 20.93 },
			comparativePriceData: { b2cPrice: 20.93 },
			onlinePromotions: [{ type: 'FIXED_PRICE', priceData: { b2cPrice: 17.9 }, medMeraRequired: false }]
		})!;
		expect(hit).toMatchObject({ price: 17.9, oldPrice: 20.93, unitPrice: 17.9 });
		expect(hit.deal).toBeUndefined();
	});

	it('ignores MedMera member prices', () => {
		const hit = toHit({
			...base,
			onlinePromotions: [
				{
					type: 'MULTI_BUY_FIXED_PRICE',
					numberOfProductRequired: 2,
					priceData: { b2cPrice: 29 },
					medMeraRequired: true
				}
			]
		})!;
		expect(hit.deal).toBeUndefined();
		expect(hit.oldPrice).toBeUndefined();
	});
});
