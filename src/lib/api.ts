// The page's way to the backend. In the Tauri app (desktop and phones) the Rust backend
// answers through commands (src-tauri/src/lib.rs); in a browser during development
// (`bun run dev`) the same Rust code answers over HTTP (server/, proxied at /api).
import type { ChainId, NearbyStore, SearchEvent } from './types';

/** Tauri's webviews carry this; a browser tab doesn't. */
export const inApp = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

const tauri = () => import('@tauri-apps/api/core');

export type Area = { center: { lat: number; lng: number }; radiusKm: number; stores: NearbyStore[] };
export type Place = { lat: number; lng: number; label: string };
type Center = { lat: number; lng: number };
export type BackendEvent = SearchEvent | { type: 'error'; message: string };

/** A failed call's message, whichever way it came. */
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** A failed /api answer's message. Vite answers 500 with an empty body when nothing
 *  listens behind its proxy: say which half of `bun run dev` is missing. */
async function failure(res: Response): Promise<Error> {
	const text = await res.text().catch(() => '');
	if (!text && res.status >= 500)
		return new Error("The Matpris backend isn't running: start it with `bun run server` (or `bun run dev`).");
	return new Error(text || res.statusText);
}

async function http<T>(path: string): Promise<T> {
	const res = await fetch(path);
	if (!res.ok) throw await failure(res);
	return res.json();
}

const query = (p: Record<string, string | number | undefined>) =>
	Object.entries(p)
		.filter(([, v]) => v !== undefined)
		.map(([k, v]) => `${k}=${encodeURIComponent(v!)}`)
		.join('&');

/**
 * Runs a search and calls `onEvent` for each event as it arrives: the area, each
 * chain's results, a second area with every store, then `done`. Aborting `signal`
 * stops listening (and, in the app, stops the search).
 */
export async function search(
	q: string,
	center: Center,
	radiusKm: number | undefined,
	onEvent: (e: BackendEvent) => void,
	signal: AbortSignal
): Promise<void> {
	if (inApp) {
		const { invoke, Channel } = await tauri();
		const channel = new Channel<BackendEvent>();
		channel.onmessage = (e) => {
			if (!signal.aborted) onEvent(e);
		};
		signal.addEventListener('abort', () => invoke('cancel_search').catch(() => {}), { once: true });
		// Resolves when the search is done (or replaced by a newer one).
		await invoke('search', { q, lat: center.lat, lng: center.lng, radius: radiusKm ?? null, channel }).catch((e) => {
			throw new Error(message(e));
		});
		return;
	}
	const res = await fetch(`/api/search?${query({ q, lat: center.lat, lng: center.lng, radius: radiusKm })}`, { signal });
	if (!res.ok || !res.body) throw await failure(res);
	const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
	let buffer = '';
	for (;;) {
		const { value, done } = await reader.read();
		if (done) break;
		buffer += value;
		const lines = buffer.split('\n');
		buffer = lines.pop()!;
		for (const line of lines) if (line) onEvent(JSON.parse(line));
	}
}

/** The search circle around `center` and its stores. */
export async function stores(center: Center, radiusKm?: number): Promise<Area> {
	if (inApp) {
		const { invoke } = await tauri();
		return invoke<Area>('stores', { lat: center.lat, lng: center.lng, radius: radiusKm ?? null }).catch((e) => {
			throw new Error(message(e));
		});
	}
	return http(`/api/stores?${query({ lat: center.lat, lng: center.lng, radius: radiusKm })}`);
}

/** An address in Sweden → coordinates. Throws "Address not found" when there's no match. */
export async function geocode(address: string): Promise<Place> {
	if (inApp) {
		const { invoke } = await tauri();
		const place = await invoke<Place | null>('geocode', { q: address }).catch((e) => {
			throw new Error(message(e));
		});
		if (!place) throw new Error('Address not found');
		return place;
	}
	return http(`/api/geocode?${query({ q: address })}`);
}

/**
 * A product photo the page may draw into the shared image: City Gross's image server
 * doesn't allow other sites to copy its pictures, so the backend fetches them. In the
 * app that's a data: URL; in a browser, the dev server's same-origin /api/image.
 */
export async function drawableImage(src: string): Promise<string> {
	if (!inApp) return `/api/image?${query({ src })}`;
	const { invoke } = await tauri();
	return invoke<string>('image', { src });
}

export type { ChainId };
