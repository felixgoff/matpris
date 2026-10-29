import { describe, expect, it } from 'vitest';
import { matchQuality, parseSek, parseSize, parseUnitPrice, unitPriceFromSize } from './normalize';

describe('parseSek', () => {
	it('handles Swedish formatting', () => {
		expect(parseSek('16,50 kr')).toBe(16.5);
		expect(parseSek('1 299,00 kr')).toBe(1299);
		expect(parseSek('72.9')).toBe(72.9);
		expect(parseSek('')).toBeUndefined();
	});
});

describe('parseUnitPrice', () => {
	it('reads kr/unit strings', () => {
		expect(parseUnitPrice('299,50 kr/kg')).toEqual({ unitPrice: 299.5, unit: 'kg' });
		expect(parseUnitPrice('117,16 kr/l')).toEqual({ unitPrice: 117.16, unit: 'l' });
		expect(parseUnitPrice('nothing')).toBeUndefined();
	});
});

describe('parseSize', () => {
	it('converts to kg / l', () => {
		expect(parseSize('500 g')).toEqual({ amount: 0.5, unit: 'kg' });
		expect(parseSize('1,5l')).toEqual({ amount: 1.5, unit: 'l' });
		expect(parseSize('6 x 33 cl')?.amount).toBeCloseTo(1.98);
		expect(parseSize('ca 1 kg')).toEqual({ amount: 1, unit: 'kg' });
	});
	it('computes unit price', () => {
		expect(unitPriceFromSize(59.9, '200 g')).toEqual({ unitPrice: 299.5, unit: 'kg' });
	});
});

describe('matchQuality', () => {
	it('matches whole words and compound heads strongly', () => {
		expect(matchQuality('mjölk', 'Mellanmjölk 1,5%')).toBe(2);
		expect(matchQuality('ost', 'Brieost')).toBe(2);
		expect(matchQuality('banan', 'Bananer Eko')).toBe(2);
		expect(matchQuality('fanta', 'Orange Zero Läsk Pet', 'Fanta')).toBe(2);
		expect(matchQuality('kaffe bryggkaffe', 'Bryggkaffe Mellanrost')).toBe(2);
	});
	it('rates longer words that only start with the query as weak', () => {
		expect(matchQuality('fanta', 'Köttbuljong Pulver', 'Fantasty')).toBe(1);
		expect(matchQuality('kaff', 'Bryggkaffe', 'Zoégas')).toBe(0);
		expect(matchQuality('kaffe', 'Kaffefilter')).toBe(1);
	});
	it('rejects unrelated products', () => {
		expect(matchQuality('banan', 'Päron')).toBe(0);
		expect(matchQuality('ost', 'Postej')).toBe(0);
		expect(matchQuality('ä', 'Päron')).toBe(0);
	});
});
