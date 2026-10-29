import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fetchJson, SupersededError } from './http';

describe('fetchJson request spacing', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.stubGlobal(
			'fetch',
			vi.fn(async (url: string) => new Response(JSON.stringify(url)))
		);
	});
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it('lets the newest latestOnly request take over a queued turn', async () => {
		const opts = { intervalMs: 10_000, maxWaitMs: 30_000, latestOnly: true };
		const first = fetchJson('https://newest.test/1', opts); // goes now
		const second = fetchJson('https://newest.test/2', opts).catch((e) => e); // queued for t=10 s
		const third = fetchJson('https://newest.test/3', opts); // replaces the queued one

		await expect(first).resolves.toBe('https://newest.test/1');
		expect(await second).toBeInstanceOf(SupersededError);
		await vi.advanceTimersByTimeAsync(9_999);
		expect(fetch).toHaveBeenCalledTimes(1); // still one Crawl-delay apart
		await vi.advanceTimersByTimeAsync(1);
		await expect(third).resolves.toBe('https://newest.test/3');
		expect(fetch).toHaveBeenCalledTimes(2);
	});

	it('queues ordinary requests behind each other', async () => {
		const opts = { intervalMs: 10_000, maxWaitMs: 30_000 };
		const a = fetchJson('https://queue.test/a', opts);
		const b = fetchJson('https://queue.test/b', opts);
		await expect(a).resolves.toBe('https://queue.test/a');
		await vi.advanceTimersByTimeAsync(10_000);
		await expect(b).resolves.toBe('https://queue.test/b');
		expect(fetch).toHaveBeenCalledTimes(2);
	});
});
