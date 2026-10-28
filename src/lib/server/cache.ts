// Small in-memory TTL cache with in-flight de-duplication: concurrent callers
// asking for the same key share one upstream request.

interface Entry<T> {
	expires: number;
	value: Promise<T>;
}

export class TtlCache<T> {
	private entries = new Map<string, Entry<T>>();

	constructor(
		private ttlMs: number,
		private maxEntries = 1000
	) {}

	get(key: string, load: () => Promise<T>): Promise<T> {
		const now = Date.now();
		const hit = this.entries.get(key);
		if (hit && hit.expires > now) return hit.value;

		const value = load();
		this.entries.set(key, { expires: now + this.ttlMs, value });
		// Failures must not be cached.
		value.catch(() => this.entries.delete(key));

		if (this.entries.size > this.maxEntries) {
			// Map keeps insertion order, so the first key is the oldest.
			this.entries.delete(this.entries.keys().next().value!);
		}
		return value;
	}
}
