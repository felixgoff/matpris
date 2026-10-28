// Address → coordinates via OpenStreetMap Nominatim (max 1 req/s per their usage policy).
import { error, json } from '@sveltejs/kit';
import { TtlCache } from '../../../lib/server/cache';
import { fetchJson } from '../../../lib/server/http';
import type { RequestHandler } from './$types';

type Place = { lat: number; lng: number; label: string };
const cache = new TtlCache<Place | null>(7 * 24 * 60 * 60 * 1000);

export const GET: RequestHandler = async ({ url }) => {
	const q = url.searchParams.get('q')?.trim().toLowerCase();
	if (!q) error(400, 'q is required');
	const place = await cache.get(q, async () => {
		const res = await fetchJson<{ lat: string; lon: string; display_name: string }[]>(
			`https://nominatim.openstreetmap.org/search?format=json&limit=1&countrycodes=se&q=${encodeURIComponent(q)}`,
			{ intervalMs: 1100 }
		);
		return res[0] ? { lat: Number(res[0].lat), lng: Number(res[0].lon), label: res[0].display_name } : null;
	});
	if (!place) error(404, 'Hittade inte adressen');
	return json(place);
};
