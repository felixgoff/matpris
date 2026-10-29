import type { Unit } from '../types';

/** "1 299,50 kr" → 1299.5 */
export function parseSek(text: string | null | undefined): number | undefined {
	if (!text) return undefined;
	const m = text.replace(/[\s ]/g, '').match(/-?\d+(?:[.,]\d+)?/);
	return m ? Number(m[0].replace(',', '.')) : undefined;
}

const UNIT_ALIASES: Record<string, Unit> = { kg: 'kg', l: 'l', liter: 'l', lit: 'l', st: 'st', styck: 'st' };

export function toUnit(text: string | null | undefined): Unit | undefined {
	return text ? UNIT_ALIASES[text.trim().toLowerCase()] : undefined;
}

/** "299,50 kr/kg" → { unitPrice: 299.5, unit: 'kg' } */
export function parseUnitPrice(text: string | null | undefined): { unitPrice: number; unit: Unit } | undefined {
	const m = text?.match(/([\d\s .,]+)\s*kr\s*\/\s*([a-zA-Z]+)/);
	const unit = toUnit(m?.[2]);
	const unitPrice = parseSek(m?.[1]);
	return unit && unitPrice !== undefined ? { unitPrice, unit } : undefined;
}

/** "500 g" → { amount: 0.5, unit: 'kg' }, "6 x 33 cl" → { amount: 1.98, unit: 'l' } */
export function parseSize(text: string | null | undefined): { amount: number; unit: Unit } | undefined {
	const m = text
		?.toLowerCase()
		.replace(',', '.')
		.match(/(?:(\d+)\s*x\s*)?(\d+(?:\.\d+)?)\s*(kg|g|l|dl|cl|ml|st)\b/);
	if (!m) return undefined;
	const count = m[1] ? Number(m[1]) : 1;
	const n = Number(m[2]) * count;
	const factor: Record<string, [number, Unit]> = {
		kg: [1, 'kg'], g: [0.001, 'kg'], l: [1, 'l'], dl: [0.1, 'l'], cl: [0.01, 'l'], ml: [0.001, 'l'], st: [1, 'st']
	};
	const [f, unit] = factor[m[3]];
	return n > 0 ? { amount: n * f, unit } : undefined;
}

export function unitPriceFromSize(price: number, size: string | undefined) {
	const s = parseSize(size);
	return s ? { unitPrice: Math.round((price / s.amount) * 100) / 100, unit: s.unit } : undefined;
}

const tokens = (s: string) => s.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);

// Swedish plural/definite endings, so "banan" matches "bananer" and "mjölk" "mjölken".
const ENDINGS = ['', 'r', 'n', 's', 't', 'er', 'ar', 'or', 'en', 'et', 'na', 'erna', 'arna', 'orna'];

export type MatchQuality = 0 | 1 | 2;

function wordQuality(q: string, word: string): MatchQuality {
	for (const end of ENDINGS) {
		const form = q + end;
		// Whole word, or the head of a compound ("mellanmjölk", "brieost").
		if (word === form || (q.length >= 3 && word.endsWith(form))) return 2;
	}
	// Merely starts with it ("fanta" → "fantasty", "kaffe" → "kaffefilter"): weak.
	return word.startsWith(q) ? 1 : 0;
}

/**
 * How well a product matches the query: 2 = every query word matches strongly,
 * 1 = some only weakly (prefix of a longer word), 0 = no match.
 */
export function matchQuality(query: string, ...fields: (string | undefined)[]): MatchQuality {
	const words = tokens(fields.filter(Boolean).join(' '));
	let worst: MatchQuality = 2;
	for (const q of tokens(query)) {
		let best: MatchQuality = 0;
		for (const w of words) if ((best = Math.max(best, wordQuality(q, w)) as MatchQuality) === 2) break;
		if (best < worst) worst = best;
		if (worst === 0) break;
	}
	return worst;
}
