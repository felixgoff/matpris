import { error } from '@sveltejs/kit';

export function readCenter(url: URL) {
	const lat = Number(url.searchParams.get('lat'));
	const lng = Number(url.searchParams.get('lng'));
	// Roughly Sweden's bounding box.
	if (!(lat > 55 && lat < 69.1 && lng > 10.5 && lng < 24.2)) error(400, 'lat/lng must be a location in Sweden');
	return { lat, lng };
}
