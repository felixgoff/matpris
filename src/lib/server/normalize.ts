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

/**
 * Does a product plausibly match the query? Every query word must appear in the
 * product text. Words of 3+ letters may appear inside compounds ("mjölk" matches
 * "Mellanmjölk"); shorter words must start a word.
 */
export function matchesQuery(query: string, ...fields: (string | undefined)[]): boolean {
	const text = fields.filter(Boolean).join(' ').toLowerCase();
	const words = tokens(text);
	return tokens(query).every((q) => (q.length >= 3 ? text.includes(q) : words.some((w) => w.startsWith(q))));
}
