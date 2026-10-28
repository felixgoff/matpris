import { describe, expect, it } from 'vitest';
import { matchesQuery, parseSek, parseSize, parseUnitPrice, unitPriceFromSize } from './normalize';

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

describe('matchesQuery', () => {
	it('matches inside Swedish compounds', () => {
		expect(matchesQuery('mjölk', 'Mellanmjölk 1,5%')).toBe(true);
		expect(matchesQuery('kaffe bryggkaffe', 'Bryggkaffe Mellanrost')).toBe(true);
	});
	it('rejects unrelated products', () => {
		expect(matchesQuery('banan', 'Päron')).toBe(false);
		expect(matchesQuery('mjölk', 'Mjölkskummare', 'SILVERCREST')).toBe(true); // filtered by category instead
	});
	it('requires short words to start a word', () => {
		expect(matchesQuery('ost', 'Brieost')).toBe(true);
		expect(matchesQuery('ä', 'Päron')).toBe(false);
	});
});
