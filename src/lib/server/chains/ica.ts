// ICA's shop API works over plain server-side HTTP — the hard part is its AWS WAF,
// which issues a short-lived aws-waf-token cookie (a few minutes of validity; the ICA
// spider in matval kept its session alive the same way, reporting 33k requests with
// plain HTTP after periodic token minting). scripts/ica-warm.mjs opens the shop in
// the persistent browser profile (scripts/.ica-profile/, or headless from a clean IP)
// and saves the fresh cookies to .ica-profile/session.json. This adapter reads them,
// and when they are stale or the WAF challenges us (HTTP 202/403), it re-runs the
// warm script once and retries. A visual challenge can only be cleared by a human in
// a headed window (`HEADED=1 bun run warm:ica`) — nothing here defeats the WAF.
//
// Prices anchor to one store (Maxi ICA Stormarknad Gunnesbo, Lund), presented like
// Lidl's list: chain pricing, a good approximation for nearby stores ranked with it.
import { execFile } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { promisify } from 'node:util';
import type { ProductHit, Store, Unit } from '../../types';
import icaStores from '../data/ica-stores.json';
import { fetchJson, ThrottledError, USER_AGENT } from '../http';
import type { ChainAdapter } from './types';

const STORE_ID = '1088004';
const BASE = 'https://handlaprivatkund.ica.se';
// Match the browser identification the session was minted with: a different UA on a
// cookie-bearing request is exactly what WAFs look at.
const BROWSER_UA =
	'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36';

const SESSION_FILE =
	process.env.ICA_SESSION_FILE ??
	new URL('../../../../scripts/.ica-profile/session.json', import.meta.url).pathname;
const WARM_SCRIPT = new URL('../../../../scripts/ica-warm.mjs', import.meta.url).pathname;

/** WAF tokens outlive a session, but matval refreshed every ~4 min. */
const STALE_MS = 3 * 60 * 1000;
/** Only until the next search pair; real crumbs live in the browser profile. */
const WARM_GAP_MS = 60 * 1000;

interface IcaSession {
	storedAt: number;
	cookies: { name: string; value: string }[];
}

interface Money {
	amount?: string | number;
	currency?: string;
}

export interface IcaProduct {
	productId: string;
	retailerProductId: string;
	type?: string;
	name: string;
	brand?: string;
	packSizeDescription?: string;
	available?: boolean;
	price?: Money;
	unitPrice?: { price?: Money; unitName?: string };
	promoPrice?: Money;
	promoUnitPrice?: { price?: Money; unitName?: string };
	depositPrice?: Money;
	image?: { src?: string };
	categoryPath?: string[];
}

interface IcaSearchResponse {
	productGroups?: {
		decoratedProducts?: IcaProduct[];
		// Group shapes elsewhere come wrapped: { productId, product }
		products?: { product: IcaProduct }[];
	}[];
}

export interface CookieJar {
	read(): Promise<IcaSession | undefined>;
	warm(): Promise<void>;
	lastWarm(): number;
}

const num = (x?: string | number) => (x == null ? undefined : Number(x));

/** "PER_LITRE_WITHOUT_DEPOSIT" | "PER_1KG" | "EACH" (also accepts the fop form). */
const unitOf = (name?: string): Unit | undefined => {
	const t = name?.toLowerCase() ?? '';
	return t.includes('kg')
		? 'kg'
		: t.includes('litre') || t.includes('liter')
			? 'l'
			: t.includes('each')
				? 'st'
				: undefined;
};

/** "Läsk Orange 1,5l Fanta" → "läsk-orange-1-5l-fanta" (matches the real PDP URLs). */
const slug = (name: string) =>
	name
		.toLowerCase()
		// Punctuation and separators collapse to one hyphen; letters (incl. å, ä, ö) and digits stay.
		.replace(/[^\p{L}\p{N}]+/gu, '-')
		.replace(/^-+|-+$/g, '');

export function toHit(p: IcaProduct): ProductHit {
	const regular = num(p.price?.amount);
	const deal = num(p.promoPrice?.amount);
	// A full-price promo (deal ≥ regular) is not a deal.
	const isDeal = deal != null && regular != null && deal < regular;
	const unitPrice = isDeal ? num(p.promoUnitPrice?.price?.amount) : num(p.unitPrice?.price?.amount);
	const unit = unitOf((isDeal ? p.promoUnitPrice : p.unitPrice)?.unitName);
	return {
		chain: 'ica',
		id: p.productId,
		name: p.name,
		brand: p.brand,
		size: p.packSizeDescription,
		price: isDeal ? deal : (regular ?? 0),
		oldPrice: isDeal ? regular : undefined,
		unitPrice,
		unit,
		categories: p.categoryPath?.slice(0, 2),
		image: p.image?.src,
		url: `${BASE}/stores/${STORE_ID}/products/${slug(p.name)}/${p.retailerProductId}`,
		pricing: 'chain'
	};
}

const jarWarm: { pending?: Promise<void>; lastWarmAt?: number } = {};

/** ICA adapter with a swappable cookie jar (all browser-related state stays here). */
export function makeIca(jar: CookieJar): ChainAdapter {
	const cookieString = async (): Promise<string> => {
		let session = await jar.read();
		if (!session || Date.now() - session.storedAt > STALE_MS) {
			if (jar.lastWarm() > Date.now() - WARM_GAP_MS) {
				if (!session) throw new ThrottledError('ICA session warming in progress — try again shortly');
			} else {
				await jar.warm();
				session = await jar.read();
			}
			if (!session)
				throw new ThrottledError(
					'no ICA session — run `bun run warm:ica` once (or visit the shop in the browser profile)'
				);
		}
		return session.cookies.map((c) => `${c.name}=${c.value}`).join('; ');
	};

	async function api(path: string): Promise<IcaSearchResponse> {
		const url = `${BASE}/stores/${STORE_ID}${path}`;
		const headers = { 'user-agent': BROWSER_UA, cookie: await cookieString() };
		try {
			return await fetchJson(url, { intervalMs: 400, headers });
		} catch (e) {
			// 202 = WAF challenge. One automatic re-mint is worth it: a stale session
			// should not end a search, but a challenge that survives a fresh token should.
			const msg = e instanceof Error ? e.message : String(e);
			if (!/answered (202|403|405)/.test(msg)) throw e;
			await jar.warm();
			try {
				return await fetchJson(url, {
					intervalMs: 400,
					headers: { ...headers, cookie: await cookieString() }
				});
			} catch {
				throw new ThrottledError('ICA challenged us again after re-minting — the IP needs a human session');
			}
		}
	}

	return {
		chain: 'ica',
		pricing: 'chain',
		async listStores() {
			return icaStores as Store[];
		},
		async search(query) {
			const data = await api(
				`/api/webproductpagews/v6/product-pages/search?includeAdditionalPageInfo=true` +
					`&maxPageSize=48&maxProductsToDecorate=48&q=${encodeURIComponent(query)}&tag=web`
			);
			const hits: ProductHit[] = [];
			for (const group of data.productGroups ?? []) {
				for (const p of group.decoratedProducts ?? []) {
					if (p.available === false) continue;
					hits.push(toHit(p));
				}
			}
			return hits;
		}
	};
}

// Default jar: reads scripts/.ica-profile/session.json; warms by running the warm script.
const jar: CookieJar = {
	async read() {
		try {
			return JSON.parse(await readFile(SESSION_FILE, 'utf8')) as IcaSession;
		} catch {
			return undefined;
		}
	},
	lastWarm: () => jarWarm.lastWarmAt ?? 0,
	async warm() {
		if (jarWarm.pending) return jarWarm.pending;
		jarWarm.lastWarmAt = Date.now();
		jarWarm.pending = promisify(execFile)(process.execPath, [WARM_SCRIPT], { timeout: 120_000 }).then(
			() => undefined,
			() => undefined
		).finally(() => {
			jarWarm.pending = undefined;
		});
		return jarWarm.pending;
	}
};

export const ica = makeIca(jar);
