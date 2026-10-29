import { describe, expect, it } from 'vitest';
import { baseForms, matchQuality, parseSek, relevance, parseSize, parseUnitPrice, unitPriceFromSize } from './normalize';

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

describe('baseForms', () => {
	it('undoes plural and definite endings', () => {
		expect(baseForms('bananer')).toContain('banan');
		expect(baseForms('gurkor')).toContain('gurka');
		expect(baseForms('äpplen')).toContain('äpple');
		expect(baseForms('mjölken')).toContain('mjölk');
		expect(baseForms('ägg')).toEqual([]);
	});
});

describe('matchQuality with inflected queries', () => {
	it('finds the base form when the query is plural', () => {
		expect(matchQuality('bananer', 'Banan Klass 1')).toBe(2);
		expect(matchQuality('tomater', 'Tomat Klass 1')).toBe(2);
		expect(matchQuality('gurkor', 'Gurka Klass 1')).toBe(2);
	});
	it('keeps "banan" a weak match for English "Banana"', () => {
		expect(matchQuality('banan', 'Banana Skids')).toBe(1);
	});
});

describe('relevance', () => {
	it('ranks the product itself above things it flavours', () => {
		expect(relevance('banan', 'Banan Klass 1')).toBe('best');
		expect(relevance('bananer', 'Banan Eko Klass 1')).toBe('best');
		expect(relevance('banan', 'Banan & Vanilj Yo-ghurt 0,1%')).toBe('mention');
		expect(relevance('banan', 'Mango Banan Apelsin Drickyoghurt')).toBe('mention');
		expect(relevance('banan', 'Fruktsmoothie Äpple & Banan Från 6 Månader')).toBe('mention');
		expect(relevance('banan', 'Bananchips')).toBe('weak');
		expect(relevance('bananer', 'Jordgubb Skumbananer Sura')).toBe('mention');
		expect(relevance('skumbanan', 'Skumbanan')).toBe('best');
		expect(relevance('banan', 'Päron')).toBeUndefined();
	});
	it('treats a product type the query asked for as the product', () => {
		expect(relevance('banan yoghurt', 'Banan & Vanilj Yo-ghurt 0,1%')).toBe('best');
		expect(relevance('mjölk', 'Mellanmjölk Längre Hållbarhet 1,5%')).toBe('best');
		expect(relevance('kaffe', 'Originalet Mellanrost Bryggkaffe')).toBe('best');
	});
});

describe('relevance with chain categories', () => {
	const coffee = ['Skafferiet', 'Kaffe, te & choklad', 'Bryggkaffe'];
	it('uses the most specific category as part of the name', () => {
		expect(relevance('kaffe', 'Originalet Mellanrost', 'Gevalia', coffee)).toBe('best');
		expect(relevance('banan', 'Banan Eko', undefined, ['Frukt & grönt', 'Frukt', 'Bananer'])).toBe('best');
		expect(relevance('kaffe', 'Earl Grey', 'Twinings', ['Skafferiet', 'Kaffe, te & choklad', 'Te'])).toBeUndefined();
	});
	it('folds sweets, yoghurt and baby food away by their shelf', () => {
		expect(relevance('banan', 'Chokladbanan', 'Aroma', ['Godis', 'Choklad', 'Chokladpåsar'])).toBe('mention');
		expect(relevance('banan', 'Jordgubb Banan', 'Danonino', ['Mejeri, ost & ägg', 'Yoghurt', 'Smaksatt yoghurt'])).toBe(
			'mention'
		);
		expect(relevance('banan', 'Mango & Banan Puré', 'Semper', ['Barn', 'Barnmat', 'BARNMAT, MAT FÖR BARN'])).toBe(
			'mention'
		);
		expect(relevance('banan', 'Banan Skivad', 'Eldorado', ['Fryst', 'Frysta bär & frukt', 'DJ FRUKT/BÄR/JUICE'])).toBe(
			'best'
		);
	});
});
