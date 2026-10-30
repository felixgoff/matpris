import { error } from '@sveltejs/kit';
import { readCenter, readRadius } from '../../../lib/server/params';
import { search } from '../../../lib/server/search';
import type { RequestHandler } from './$types';

/** Streams newline-delimited JSON SearchEvents so fast chains render first. */
export const GET: RequestHandler = ({ url }) => {
	const q = url.searchParams.get('q')?.trim();
	if (!q || q.length < 2) error(400, 'q must be at least 2 characters');
	const center = readCenter(url);

	const encoder = new TextEncoder();
	const events = search(q, center, readRadius(url));
	const stream = new ReadableStream({
		async pull(controller) {
			try {
				const { value, done } = await events.next();
				if (done) controller.close();
				else controller.enqueue(encoder.encode(JSON.stringify(value) + '\n'));
			} catch (e) {
				controller.enqueue(encoder.encode(JSON.stringify({ type: 'error', message: (e as Error).message }) + '\n'));
				controller.close();
			}
		},
		cancel() {
			events.return(undefined);
		}
	});
	return new Response(stream, {
		headers: { 'content-type': 'application/x-ndjson; charset=utf-8', 'cache-control': 'no-store' }
	});
};
