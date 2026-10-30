import { json } from '@sveltejs/kit';
import { readCenter, readRadius } from '../../../lib/server/params';
import { pricedArea, withMapOnlyStores } from '../../../lib/server/stores/registry';
import type { RequestHandler } from './$types';

export const GET: RequestHandler = async ({ url }) => {
	const center = readCenter(url);
	return json({ center, ...(await withMapOnlyStores(center, await pricedArea(center, readRadius(url)))) });
};
