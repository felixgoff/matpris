import { describe, expect, it } from 'vitest';
import { toHit, type AxfoodProduct } from './axfood';

// Shapes taken from real willys.se search results.
const base: AxfoodProduct = {
	code: '101233933_ST',
	name: 'Orange Zero Läsk Pet',
	manufacturer: 'Fanta',
	displayVolume: '1,5l',
	priceValue: 19.4,
	comparePrice: '12,93 kr',
	comparePriceUnit: 'l',
	outOfStock: false,
	image: null,
	potentialPromotions: []
};

describe('Axfood toHit', () => {
	it('maps a regular price', () => {
		const hit = toHit('willys', 'https://www.willys.se', base);
		expect(hit).toMatchObject({ price: 19.4, unitPrice: 12.93, unit: 'l' });
		expect(hit.deal).toBeUndefined();
		expect(hit.oldPrice).toBeUndefined();
	});

	it('estimates the price of one piece for products sold by weight', () => {
		const hit = toHit('willys', 'https://www.willys.se', {
			...base,
			name: 'Banan Klass 1',
			displayVolume: 'ca: 180g',
			priceValue: 19.9,
			priceUnit: 'kr/kg',
			averageWeight: 0.18,
			comparePrice: '19,90 kr',
			comparePriceUnit: 'kg'
		});
		expect(hit).toMatchObject({ price: 19.9, perKg: true, approxEach: 3.58, unitPrice: 19.9, unit: 'kg' });
	});

	it('maps a multi-buy "5 för 55 +pant" deal', () => {
		const hit = toHit('willys', 'https://www.willys.se', {
			...base,
			potentialPromotions: [
				{
					qualifyingCount: 5,
					rewardLabel: '55,00 +pant',
					price: { value: 11 },
					comparePrice: '7,33 kr',
					promotionType: 'MixMatchPricePromotion',
					redeemLimitLabel: 'Max 5 köp'
				}
			]
		});
		expect(hit.price).toBe(19.4); // buying one still costs the regular price
		expect(hit.deal).toEqual({
			quantity: 5,
			each: 11,
			total: 55,
			unitPrice: 7.33,
			mixAndMatch: true,
			plusDeposit: true,
			limit: 5
		});
	});

	it('maps a single-item sale as the current price', () => {
		const hit = toHit('willys', 'https://www.willys.se', {
			...base,
			priceValue: 79.9,
			comparePrice: '177,56 kr',
			comparePriceUnit: 'kg',
			potentialPromotions: [
				{
					qualifyingCount: 1,
					rewardLabel: '72,90/st',
					price: { value: 72.9 },
					comparePrice: '162,00 kr',
					promotionType: 'MixMatchPricePromotion',
					redeemLimitLabel: ''
				}
			]
		});
		expect(hit).toMatchObject({ price: 72.9, oldPrice: 79.9, unitPrice: 162 });
		expect(hit.deal).toBeUndefined();
	});
});
