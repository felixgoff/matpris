import { error } from '@sveltejs/kit';

export function readCenter(url: URL) {
	const lat = Number(url.searchParams.get('lat'));
	const lng = Number(url.searchParams.get('lng'));
	// Roughly Sweden's bounding box.
	if (!(lat > 55 && lat < 69.1 && lng > 10.5 && lng < 24.2)) error(400, 'lat/lng must be a location in Sweden');
	return { lat, lng };
}

/** An optional search radius the shopper picked, in km; absent means automatic. */
export function readRadius(url: URL): number | undefined {
	const raw = url.searchParams.get('radius');
	if (!raw) return undefined;
	const km = Number(raw);
	if (!(km >= 0.5 && km <= 30)) error(400, 'radius must be between 0.5 and 30 km');
	return km;
}
