import { describe, expect, it } from 'vitest';
import { toHit, type LidlItem } from './lidl';

// Shapes taken from real lidl.se search results (trimmed).
const regular: LidlItem = {
	code: '10001',
	gridbox: {
		data: {
			fullTitle: 'Chokladmjölk',
			category: 'Food',
			price: { price: 17.9, basePrice: { text: '29,83 kr/l' }, packaging: { text: '600 ml' } }
		}
	}
};

// On a Lidl Plus offer: no plain price, only the member price and the price before it.
const lidlPlus: LidlItem = {
	code: '10002',
	gridbox: {
		data: {
			fullTitle: 'MATRIKET Standardmjölk med längre hållbarhet',
			brand: { name: 'MATRIKET' },
			category: 'Food',
			price: {},
			regionsV2: { '1': { isDefault: true, regionPriceId: '1' }, '2': { isDefault: false, regionPriceId: '1' } },
			regionsPrices: {
				'1': {
					currentLidlPlusPrice: {
						price: { price: 12.9, oldPrice: 15.9, basePrice: { text: '8,60 kr/l' }, packaging: { text: '1,5 l' } }
					}
				}
			}
		}
	}
};

describe('Lidl toHit', () => {
	it('maps a regular price', () => {
		expect(toHit(regular)).toMatchObject({ price: 17.9, unitPrice: 29.83, unit: 'l', size: '600 ml' });
	});

	it("uses the non-member price of a Lidl Plus offer, with its own unit price", () => {
		const hit = toHit(lidlPlus);
		expect(hit).toMatchObject({
			name: 'Standardmjölk med längre hållbarhet',
			brand: 'MATRIKET',
			price: 15.9,
			unit: 'l',
			size: '1,5 l',
			oldPrice: undefined
		});
		expect(hit?.unitPrice).toBeCloseTo(10.6, 2);
	});

	it('skips items without any price and non-food', () => {
		const bare = { ...lidlPlus, gridbox: { data: { ...lidlPlus.gridbox!.data, regionsPrices: {} } } };
		expect(toHit(bare)).toBeUndefined();
		expect(toHit({ ...regular, gridbox: { data: { ...regular.gridbox!.data, category: 'NonFood' } } })).toBeUndefined();
	});
});
