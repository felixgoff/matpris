<script lang="ts">
	import { onMount } from 'svelte';
	import { loadMapKit } from '../mapkit';
	import { CHAINS, type NearbyStore } from '../types';

	interface Props {
		center: { lat: number; lng: number };
		radiusKm?: number;
		stores: NearbyStore[];
		/** Store id to emphasise (the cheapest one). */
		highlight?: string;
		/** Draw our own pin at `center`; off when MapKit already shows the device location there. */
		showCenterPin?: boolean;
		/** Called with each device location MapKit reports. */
		onlocate?: (position: { lat: number; lng: number }) => void;
		onlocateerror?: (message: string) => void;
	}
	let { center, radiusKm, stores, highlight, showCenterPin = true, onlocate, onlocateerror }: Props = $props();

	let el: HTMLDivElement;
	let map = $state<mapkit.Map>();
	let failed = $state('');
	/** What the view is fitted to; refitted when the map changes size. */
	let fitted: (mapkit.Annotation | mapkit.Overlay)[] = [];

	// Every store lies inside the search circle, so fitting the circle is enough as
	// long as the padding leaves room for a pin at its edge: a pin's height above,
	// half the widest (highlighted) pin's width at the sides.
	const padding = () => new mapkit.Padding(48, 56, 12, 56);

	onMount(() => {
		let cancelled = false;
		let created: mapkit.Map | undefined;
		const resize = new ResizeObserver(() => {
			if (created && fitted.length) created.showItems(fitted, { animate: false, padding: padding() });
		});

		loadMapKit().then(
			(mk) => {
				if (cancelled) return;
				created = new mk.Map(el, {
					showsPointsOfInterest: false,
					showsMapTypeControl: false,
					isRotationEnabled: false
				});
				created.addEventListener('user-location-change', (e: { coordinate: mapkit.Coordinate }) =>
					onlocate?.({ lat: e.coordinate.latitude, lng: e.coordinate.longitude })
				);
				created.addEventListener('user-location-error', (e: { message?: string }) =>
					onlocateerror?.(e.message || 'platsåtkomst nekad')
				);
				// Asks the browser for the location and draws MapKit's blue dot. We don't
				// set tracksUserLocation: the view should fit the stores, not follow the dot.
				created.showsUserLocation = true;
				map = created;
				resize.observe(el);
			},
			(e: Error) => (failed = e.message)
		);
		return () => {
			cancelled = true;
			resize.disconnect();
			created?.destroy();
		};
	});

	/** Ask MapKit for the device location again. Returns false if the map isn't available. */
	export function locate(): boolean {
		if (!map) return false;
		const known = map.userLocationAnnotation?.coordinate;
		if (known) onlocate?.({ lat: known.latitude, lng: known.longitude });
		else {
			map.showsUserLocation = false;
			map.showsUserLocation = true;
		}
		return true;
	}

	function pinElement(s: NearbyStore, hot: boolean) {
		const pin = document.createElement('div');
		pin.className = hot ? 'store-pin hot' : 'store-pin';
		pin.style.setProperty('--chain', CHAINS[s.chain].color);
		const logo = document.createElement('img');
		logo.src = CHAINS[s.chain].logo;
		logo.alt = CHAINS[s.chain].name;
		logo.className = s.chain;
		pin.append(logo);
		if (hot) {
			const star = document.createElement('span');
			star.className = 'star';
			star.textContent = '★';
			pin.append(star);
		}
		return pin;
	}

	$effect(() => {
		if (!map) return;
		const at = (lat: number, lng: number) => new mapkit.Coordinate(lat, lng);
		map.removeOverlays(map.overlays);
		map.removeAnnotations(map.annotations);

		const items: (mapkit.Annotation | mapkit.Overlay)[] = [];
		if (radiusKm) {
			const circle = new mapkit.CircleOverlay(at(center.lat, center.lng), radiusKm * 1000, {
				style: new mapkit.Style({
					strokeColor: '#1a7f4b',
					lineWidth: 2,
					lineDash: [6, 4],
					fillColor: '#1a7f4b',
					fillOpacity: 0.06
				})
			});
			map.addOverlay(circle);
			items.push(circle);
		}

		// MapKit hides colliding annotations below "required" priority, and nearby
		// stores always collide, so every pin is required.
		const required = mapkit.Annotation.DisplayPriority.Required;
		// The cheapest store goes last so it is drawn on top.
		const ordered = stores.toSorted((a, b) => Number(a.id === highlight) - Number(b.id === highlight));
		const annotations: mapkit.Annotation[] = ordered.map(
			(s) =>
				new mapkit.Annotation(at(s.lat, s.lng), () => pinElement(s, s.id === highlight), {
					title: s.name,
					subtitle: `${s.address} · ${s.distanceKm.toLocaleString('sv-SE', { maximumFractionDigits: 1 })} km bort`,
					displayPriority: required,
					// Pins are anchored at their bottom centre; lift them by the pointer's height.
					anchorOffset: new DOMPoint(0, -7)
				})
		);
		if (showCenterPin) {
			annotations.unshift(
				new mapkit.MarkerAnnotation(at(center.lat, center.lng), {
					color: '#1b1f1d',
					title: 'Vald plats',
					glyphText: '•',
					displayPriority: required,
					titleVisibility: mapkit.FeatureVisibility.Hidden
				})
			);
		}
		map.addAnnotations(annotations);
		items.push(...annotations);

		// Fit the circle when there is one; MapKit's own fit of the wide pins zooms out too far.
		fitted = items[0] instanceof mapkit.CircleOverlay ? [items[0]] : items;
		// Not animated: overlapping animations (location fix + search results) can end at the wrong zoom.
		map.showItems(fitted, { animate: false, padding: padding() });
	});
</script>

<div class="map" bind:this={el}>
	{#if failed}<p class="failed">Kartan kunde inte laddas: {failed}</p>{/if}
</div>

<style>
	.map {
		height: 100%;
		min-height: 320px;
		border-radius: 12px;
		overflow: hidden;
		position: relative;
		background: #e8ebe9;
	}
	.failed {
		position: absolute;
		inset: 0;
		margin: auto;
		height: fit-content;
		text-align: center;
		color: #555;
		padding: 16px;
	}
	/* Pins are created by MapKit outside this component's markup, hence :global. */
	:global(.store-pin) {
		position: relative;
		display: flex;
		align-items: center;
		height: 24px;
		padding: 0 5px;
		box-sizing: border-box;
		background: #fff;
		border: 2px solid var(--chain);
		border-radius: 7px;
		box-shadow: 0 1px 4px #0005;
		cursor: pointer;
	}
	:global(.store-pin::after) {
		content: '';
		position: absolute;
		left: 50%;
		bottom: -7px;
		transform: translateX(-50%);
		border: 6px solid transparent;
		border-top-color: var(--chain);
		border-bottom: 0;
	}
	:global(.store-pin img) {
		display: block;
		height: 11px;
	}
	:global(.store-pin img.lidl) {
		height: 18px;
	}
	:global(.store-pin:has(img.lidl)) {
		padding: 0 1px;
	}
	:global(.store-pin.hot) {
		height: 34px;
		padding: 0 8px;
		border: 3px solid #1a7f4b;
		box-shadow: 0 2px 10px #1a7f4b88;
	}
	:global(.store-pin.hot::after) {
		border-top-color: #1a7f4b;
	}
	:global(.store-pin.hot img) {
		height: 16px;
	}
	:global(.store-pin.hot img.lidl) {
		height: 26px;
	}
	:global(.store-pin .star) {
		position: absolute;
		top: -11px;
		right: -11px;
		width: 20px;
		height: 20px;
		display: grid;
		place-items: center;
		border-radius: 50%;
		background: #1a7f4b;
		color: #fff;
		font-size: 12px;
	}
</style>
