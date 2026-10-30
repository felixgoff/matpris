// A product photo served from our own origin, for drawing the shared answer image:
// a page may only copy another site's image into a picture if that site allows it,
// and City Gross's image server doesn't. Only the chains' image hosts are fetched.
import { error } from '@sveltejs/kit';
import { USER_AGENT } from '../../../lib/server/http';
import type { RequestHandler } from './$types';

const HOSTS = ['axfood.se', 'citygross.se', 'lidl.se', 'cloudinary.com', 'ica.se', 'icanet.se'];

export const GET: RequestHandler = async ({ url }) => {
	let src: URL;
	try {
		src = new URL(url.searchParams.get('src') ?? '');
	} catch {
		error(400, 'src must be an image URL');
	}
	if (src.protocol !== 'https:' || !HOSTS.some((h) => src.hostname === h || src.hostname.endsWith(`.${h}`)))
		error(400, 'src is not a product image host');

	const res = await fetch(src, { headers: { 'user-agent': USER_AGENT }, signal: AbortSignal.timeout(8000) }).catch(
		() => undefined
	);
	const type = res?.headers.get('content-type') ?? '';
	if (!res?.ok || !type.startsWith('image/')) error(502, 'image unavailable');
	return new Response(res.body, { headers: { 'content-type': type, 'cache-control': 'public, max-age=86400' } });
};
