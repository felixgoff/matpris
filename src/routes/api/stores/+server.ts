import { json } from '@sveltejs/kit';
import { readCenter } from '../../../lib/server/params';
import { pickArea } from '../../../lib/server/stores/radius';
import { allStores } from '../../../lib/server/stores/registry';
import type { RequestHandler } from './$types';

export const GET: RequestHandler = async ({ url }) => {
	const center = readCenter(url);
	return json({ center, ...pickArea(center, await allStores()) });
};
