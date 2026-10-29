// Outbound HTTP for the chain adapters: identifies itself, times out, and
// spaces requests per host so we stay within each site's stated crawl rate.

export const USER_AGENT = 'matpris/0.1 (grocery price comparison)';

export class ThrottledError extends Error {}
/** A queued request dropped because a newer one for the same host took over its turn. */
export class SupersededError extends ThrottledError {}

const nextSlot = new Map<string, number>();
/** Per host, the `latestOnly` request still waiting for its turn. */
const waiting = new Map<string, { slot: number; drop: () => void }>();

/**
 * Reserve the next request slot for `host`. Requests are spaced `intervalMs`
 * apart; if the wait would exceed `maxWaitMs` we give up instead of queueing
 * users behind each other. A `latestOnly` request takes over the turn of one
 * that is still waiting (which fails with SupersededError) instead of queueing
 * behind it: when someone searches again, only the newest search matters.
 */
// shortcut: "newest wins" assumes one person searching at a time; with several users,
// drop a queued request when its own client disconnects instead.
async function takeSlot(host: string, intervalMs: number, maxWaitMs: number, latestOnly = false) {
	const now = Date.now();
	const queued = latestOnly ? waiting.get(host) : undefined;
	let slot: number;
	if (queued) {
		queued.drop();
		slot = queued.slot;
	} else {
		slot = Math.max(now, nextSlot.get(host) ?? 0);
		if (slot - now > maxWaitMs) throw new ThrottledError(`${host} is rate limited, try again shortly`);
		nextSlot.set(host, slot + intervalMs);
	}
	if (slot <= now) return;
	await new Promise<void>((resolve, reject) => {
		let timer: ReturnType<typeof setTimeout>;
		const entry = {
			slot,
			drop() {
				clearTimeout(timer);
				reject(new SupersededError(`${host}: replaced by a newer search`));
			}
		};
		timer = setTimeout(() => {
			if (waiting.get(host) === entry) waiting.delete(host);
			resolve();
		}, slot - now);
		if (latestOnly) waiting.set(host, entry);
	});
}

export interface FetchOptions {
	/** Minimum time between requests to the same host. */
	intervalMs: number;
	maxWaitMs?: number;
	timeoutMs?: number;
	/** Extra headers (e.g. a cookie jar a session refresher maintains). */
	headers?: Record<string, string>;
	/** A newer latestOnly request to the same host replaces this one while it waits. */
	latestOnly?: boolean;
}

export async function fetchJson<T>(url: string, opts: FetchOptions): Promise<T> {
	const host = new URL(url).host;
	await takeSlot(host, opts.intervalMs, opts.maxWaitMs ?? 8000, opts.latestOnly);
	const res = await fetch(url, {
		// Not 'application/json': Lidl's search answers 401 to that (it serves its own media type).
		headers: { 'user-agent': USER_AGENT, accept: '*/*', ...opts.headers },
		signal: AbortSignal.timeout(opts.timeoutMs ?? 8000)
	});
	if (!res.ok) throw new Error(`${host} answered ${res.status}`);
	return res.json() as Promise<T>;
}
