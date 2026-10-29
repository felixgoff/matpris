import { describe, expect, it } from 'vitest';
import { toHit, type CgProduct } from './citygross';

// Shapes taken from real citygross.se search results.
const price = (price: number, unit: string, comparativePrice: number, comparativePriceUnit: string) => ({
	price,
	unit,
	comparativePrice,
	comparativePriceUnit
});

const base: CgProduct = {
	id: '100235247_KG',
	name: 'Banan Eko',
	brand: null,
	descriptiveSize: 'CA 170G',
	url: '/matvaror/frukt-och-gront/frukt/banan-eko-p100235247_KG',
	images: [{ url: '1st_banan-600.jpg' }],
	productStoreDetails: {
		prices: {
			currentPrice: price(22.95, 'KGM', 22.95, 'KGM'),
			ordinaryPrice: price(22.95, 'KGM', 22.95, 'KGM'),
			activePromotion: null
		}
	}
};

describe('City Gross toHit', () => {
	it('maps a product sold by weight', () => {
		expect(toHit(base)).toMatchObject({
			chain: 'citygross',
			price: 22.95,
			unitPrice: 22.95,
			unit: 'kg',
			perKg: true,
			approxEach: 3.9,
			size: 'ca 170g',
			image: 'https://www.citygross.se/images/products/1st_banan-600.jpg',
			url: 'https://www.citygross.se/matvaror/frukt-och-gront/frukt/banan-eko-p100235247_KG'
		});
	});

	it('maps a "2 för 22 kr" offer', () => {
		const hit = toHit({
			...base,
			name: 'Chokladbanan',
			brand: 'AROMA',
			descriptiveSize: '100G',
			productStoreDetails: {
				prices: {
					currentPrice: price(13.5, 'PCE', 135, 'KGM'),
					ordinaryPrice: price(13.5, 'PCE', 135, 'KGM'),
					activePromotion: {
						effectType: 'ItemsTotal',
						membersOnly: false,
						minQuantity: 2,
						value: 22,
						maxAppliedPerReceipt: 0,
						priceDetails: price(11, 'PCE', 110, 'KGM')
					}
				}
			}
		});
		expect(hit).toMatchObject({ brand: 'Aroma', price: 13.5, perKg: undefined });
		expect(hit?.deal).toEqual({
			quantity: 2,
			each: 11,
			total: 22,
			unitPrice: 110,
			mixAndMatch: false,
			plusDeposit: false,
			limit: undefined
		});
	});

	it('leaves out members-only offers and products without a price', () => {
		const members = toHit({
			...base,
			productStoreDetails: {
				prices: {
					currentPrice: price(13.5, 'PCE', 135, 'KGM'),
					ordinaryPrice: null,
					activePromotion: {
						effectType: 'ItemsTotal',
						membersOnly: true,
						minQuantity: 2,
						value: 22,
						maxAppliedPerReceipt: 0,
						priceDetails: price(11, 'PCE', 110, 'KGM')
					}
				}
			}
		});
		expect(members?.deal).toBeUndefined();
		expect(toHit({ ...base, productStoreDetails: null })).toBeUndefined();
	});
});
