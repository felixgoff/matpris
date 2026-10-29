// Loads Apple MapKit JS from Apple's CDN once and initialises it with the
// token from PUBLIC_MAPKIT_TOKEN (.env). Browser only.
import { PUBLIC_MAPKIT_TOKEN } from '$app/env/public';

let loading: Promise<typeof mapkit> | undefined;

export function loadMapKit(): Promise<typeof mapkit> {
	return (loading ??= new Promise<typeof mapkit>((resolve, reject) => {
		const token = PUBLIC_MAPKIT_TOKEN;
		if (!token) return reject(new Error('PUBLIC_MAPKIT_TOKEN is missing from .env'));

		const callback = '__mapkitReady';
		(window as unknown as Record<string, () => void>)[callback] = () => {
			mapkit.init({ authorizationCallback: (done) => done(token), language: 'en' });
			resolve(mapkit);
		};
		const script = document.createElement('script');
		script.src = 'https://cdn.apple-mapkit.com/mk/5.x.x/mapkit.core.js';
		script.crossOrigin = 'anonymous';
		script.dataset.libraries = 'map,annotations,overlays,user-location';
		script.dataset.callback = callback;
		script.onerror = () => reject(new Error('Apple Maps failed to load'));
		document.head.append(script);
	}).catch((e) => {
		loading = undefined; // allow a retry on the next mount
		throw e;
	}));
}
