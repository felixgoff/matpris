// Outbound HTTP for the chain adapters: identifies itself, times out, and
// spaces requests per host so we stay within each site's stated crawl rate.

export const USER_AGENT = 'matpris/0.1 (grocery price comparison)';

export class ThrottledError extends Error {}

const nextSlot = new Map<string, number>();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/**
 * Reserve the next request slot for `host`. Requests are spaced `intervalMs`
 * apart; if the wait would exceed `maxWaitMs` we give up instead of queueing
 * users behind each other.
 */
async function takeSlot(host: string, intervalMs: number, maxWaitMs: number) {
	const now = Date.now();
	const slot = Math.max(now, nextSlot.get(host) ?? 0);
	if (slot - now > maxWaitMs) throw new ThrottledError(`${host} is rate limited, try again shortly`);
	nextSlot.set(host, slot + intervalMs);
	if (slot > now) await sleep(slot - now);
}

export interface FetchOptions {
	/** Minimum time between requests to the same host. */
	intervalMs: number;
	maxWaitMs?: number;
	timeoutMs?: number;
}

export async function fetchJson<T>(url: string, opts: FetchOptions): Promise<T> {
	const host = new URL(url).host;
	await takeSlot(host, opts.intervalMs, opts.maxWaitMs ?? 8000);
	const res = await fetch(url, {
		// Not 'application/json': Lidl's search answers 401 to that (it serves its own media type).
		headers: { 'user-agent': USER_AGENT, accept: '*/*' },
		signal: AbortSignal.timeout(opts.timeoutMs ?? 8000)
	});
	if (!res.ok) throw new Error(`${host} answered ${res.status}`);
	return res.json() as Promise<T>;
}
