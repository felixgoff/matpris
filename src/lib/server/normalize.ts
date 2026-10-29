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

// Hyphens inside a word are dropped first, so "Yo-ghurt" reads as "yoghurt".
const tokens = (s: string) =>
	s
		.toLowerCase()
		.replace(/(\p{L})-(\p{L})/gu, '$1$2')
		.split(/[^\p{L}\p{N}]+/u)
		.filter(Boolean);

// Swedish plural/definite endings, so "banan" matches "bananer" and "mjölk" "mjölken".
const ENDINGS = ['', 'r', 'n', 's', 't', 'er', 'ar', 'or', 'en', 'et', 'na', 'erna', 'arna', 'orna'];

// The reverse, for a query typed in plural or definite form: each ending with what may
// replace it, so "bananer" finds "Banan", "gurkor" "Gurka" and "äpplen" "Äpple".
const INFLECTIONS: [string, string[]][] = [
	['orna', ['a']],
	['arna', ['', 'e']],
	['erna', ['']],
	['or', ['a']],
	['ar', ['', 'e']],
	['er', ['', 'e']],
	['en', ['e', '']],
	['et', ['']],
	['na', ['']],
	['n', ['']]
];

/** Possible base forms of a query word: "bananer" → ["banan", "banane"]. */
export function baseForms(word: string): string[] {
	const forms = new Set<string>();
	for (const [end, replacements] of INFLECTIONS)
		if (word.endsWith(end) && word.length - end.length >= 3)
			for (const r of replacements) forms.add(word.slice(0, -end.length) + r);
	forms.delete(word);
	return [...forms];
}

export type MatchQuality = 0 | 1 | 2;

function formQuality(q: string, word: string): MatchQuality {
	for (const end of ENDINGS) {
		const form = q + end;
		// Whole word, or the head of a compound ("mellanmjölk", "brieost").
		if (word === form || (q.length >= 3 && word.endsWith(form))) return 2;
	}
	// Merely starts with it ("fanta" → "fantasty", "kaffe" → "kaffefilter"): weak.
	return word.startsWith(q) ? 1 : 0;
}

function wordQuality(q: string, word: string): MatchQuality {
	const direct = formQuality(q, word);
	// Base forms only rescue words that don't match at all, so "banan" stays a weak
	// match for "Banana" instead of reaching it through "bana" + "na".
	return direct || (baseForms(q).some((b) => formQuality(b, word) === 2) ? 2 : 0);
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

// Words that name a kind of product. When a name has one the query didn't ask for,
// the query word is usually a flavour or an ingredient: "Banan & Vanilj Yoghurt".
// shortcut: a hand-kept list; replace it with the chains' category data if they expose it per product.
const PRODUCT_TYPES = [
	'yoghurt', 'kvarg', 'smoothie', 'juice', 'nektar', 'dryck', 'läsk', 'saft', 'glass', 'sorbet', 'gröt',
	'välling', 'puré', 'bar', 'chips', 'snacks', 'sticks', 'puffs', 'kex', 'kaka', 'kakor', 'bulle', 'bullar',
	'muffins', 'paj', 'tårta', 'godis', 'choklad', 'kola', 'sylt', 'marmelad', 'müsli', 'musli', 'granola',
	'flingor', 'pulver', 'shake', 'whey', 'snus', 'sås', 'soppa', 'dressing', 'rulle', 'toffee', 'te',
	'pudding', 'kräm', 'mjölk', 'ost', 'bröd', 'korv', 'pålägg', 'mousse', 'cream', 'split'
];
const isProductType = (word: string) =>
	// Short words only as whole words, so "te" doesn't match every word ending in "te".
	PRODUCT_TYPES.some((t) => word === t || (t.length >= 4 && word.endsWith(t)));
// Sweets named after what they look or taste like: "Skumbanan", "Chokladbanan".
const SWEET_PREFIXES = ['skum', 'choklad', 'gelé', 'lakrits', 'marsipan', 'godis'];
const BABY_FOOD = /\bfrån \d+ (?:månader|mån|år)\b|\b\d+\s*-\s*\d+\s*år\b/i;

/**
 * 'best': the product is the thing searched for ("Banan Klass 1" for "banan").
 * 'mention': the query only flavours or fills something else ("Banan & Vanilj Yoghurt", baby food).
 * 'weak': the query only starts a longer word ("Fantasty" for "fanta").
 */
export type Relevance = 'best' | 'mention' | 'weak';

/**
 * `categories` (broad to specific, e.g. ["Skafferiet", "Kaffe, te & choklad", "Bryggkaffe"])
 * come from chains that publish them (City Gross). The most specific one counts as part of
 * the name, since names there often leave out the product type ("Originalet Mellanrost").
 */
export function relevance(query: string, name: string, brand?: string, categories: string[] = []): Relevance | undefined {
	const specific = categories.at(-1);
	const quality = matchQuality(query, name, brand, specific);
	if (!quality) return undefined;
	if (quality === 1) return 'weak';
	const asked = tokens(query);
	const isAsked = (w: string) => asked.some((q) => wordQuality(q, w) === 2);
	if (specific && tokens(specific).some(isAsked)) return 'best';
	// The broader levels name the shelf ("Godis", "Barnmat"); the specific one can be a
	// buying code ("DJ FRUKT/BÄR/JUICE") that would misfire here.
	const shelf = tokens(categories.slice(0, -1).join(' ')).filter((w) => !isAsked(w));
	if (shelf.some((w) => isProductType(w) || w.startsWith('barn'))) return 'mention';
	const words = tokens(name);
	const other = words.filter((w) => !isAsked(w));
	const sweet = words.some((w) => isAsked(w) && SWEET_PREFIXES.some((p) => w.startsWith(p) && !asked.includes(w)));
	return sweet || other.some(isProductType) || BABY_FOOD.test(name) ? 'mention' : 'best';
}
