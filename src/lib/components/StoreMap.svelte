<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { km } from '../format';
	import { loadMapKit } from '../mapkit';
	import { CHAINS, type NearbyStore } from '../types';

	interface Props {
		center: { lat: number; lng: number };
		radiusKm?: number;
		stores: NearbyStore[];
		/** Store id to emphasise (the cheapest one). */
		highlight?: string;
		/** Price shown on the emphasised store's pin. */
		highlightLabel?: string;
		/** Store id the shopper is pointing at in the list; every other pin dims. */
		focus?: string;
		/** Draw our own pin at `center`; off when MapKit already shows the device location there. */
		showCenterPin?: boolean;
		/** Called with each device location MapKit reports. */
		onlocate?: (position: { lat: number; lng: number }) => void;
		onlocateerror?: (message: string) => void;
	}
	let { center, radiusKm, stores, highlight, highlightLabel, focus, showCenterPin = true, onlocate, onlocateerror }: Props =
		$props();

	let el: HTMLDivElement;
	let map = $state<mapkit.Map>();
	let failed = $state('');
	/** What the view is fitted to; refitted when the map changes size. */
	let fitted: (mapkit.Annotation | mapkit.Overlay)[] = [];
	/** Pin elements by store id, so focus changes restyle pins without rebuilding the map. */
	const pins = new Map<string, HTMLElement>();
	const focusClasses = (pin: HTMLElement, id: string) => {
		pin.classList.toggle('dim', !!focus && id !== focus);
		pin.classList.toggle('focus', id === focus);
	};

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
					onlocateerror?.(e.message || 'location access denied')
				);
				// Zooming in makes room for culled pins; zooming out culls more.
				created.addEventListener('region-change-end', cull);
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
		pin.className = ['store-pin', hot && 'hot', !CHAINS[s.chain].online && 'offline'].filter(Boolean).join(' ');
		pin.style.setProperty('--chain', CHAINS[s.chain].color);
		const logo = document.createElement('img');
		logo.src = CHAINS[s.chain].logo;
		logo.alt = CHAINS[s.chain].name;
		logo.className = s.chain;
		pin.append(logo);
		if (hot && highlightLabel) {
			const price = document.createElement('b');
			price.textContent = highlightLabel;
			pin.append(price);
		}
		pins.set(s.id, pin);
		// MapKit may build pins inside the effect below; hovering must not rebuild the map.
		untrack(() => focusClasses(pin, s.id));
		return pin;
	}

	/** The annotations culling places; `store` is unset for the location pin. */
	let placed: { a: mapkit.Annotation; store?: NearbyStore }[] = [];

	// MapKit's displayPriority/collisionMode leave custom pins piled on each other, so
	// pins are culled here. They're placed most important first: the cheapest and the
	// store the shopper points at (both always shown), the location, priced stores,
	// then map-only ones, nearest first. A pin that would overlap one already placed
	// hides until zooming in makes room for it.
	const rank = (s?: NearbyStore) =>
		!s ? 2 : s.id === highlight ? 0 : s.id === focus ? 1 : CHAINS[s.chain].online ? 3 : 4;
	function cull() {
		if (!map) return;
		const m = map;
		/** Page box of a w×h shape whose bottom centre sits `below` px under the coordinate. */
		const box = (c: mapkit.Coordinate, w: number, h: number, below = 0) => {
			const p = m.convertCoordinateToPointOnPage(c);
			return { l: p.x - w / 2 - 1, r: p.x + w / 2 + 1, t: p.y + below - h - 1, b: p.y + below + 1 };
		};
		type Box = ReturnType<typeof box>;
		const hits = (a: Box, b: Box) => a.l < b.r && b.l < a.r && a.t < b.b && b.t < a.b;
		const taken: Box[] = [];
		// MapKit's own blue dot can't be hidden, so pins make room for it.
		const me = m.userLocationAnnotation?.coordinate;
		if (me) taken.push(box(me, 22, 22, 11));
		const order = placed.toSorted(
			(x, y) => rank(x.store) - rank(y.store) || (x.store?.distanceKm ?? 0) - (y.store?.distanceKm ?? 0)
		);
		for (const { a, store } of order) {
			const pin = store && pins.get(store.id);
			if (store && !pin?.offsetWidth) continue; // not laid out yet
			// Store pins plus their pointer, or MapKit's marker balloon for the location.
			const b = pin ? box(a.coordinate, pin.offsetWidth, pin.offsetHeight + 7) : box(a.coordinate, 30, 40);
			const show = rank(store) <= 1 || !taken.some((t) => hits(b, t));
			if (show) taken.push(b);
			// A class hides our own pins at once and keeps them measurable; MapKit applies
			// `visible` only when it next redraws.
			if (pin) pin.classList.toggle('culled', !show);
			else a.visible = show;
		}
	}

	$effect(() => {
		if (!map) return;
		const at = (lat: number, lng: number) => new mapkit.Coordinate(lat, lng);
		map.removeOverlays(map.overlays);
		map.removeAnnotations(map.annotations);
		pins.clear();

		const items: (mapkit.Annotation | mapkit.Overlay)[] = [];
		if (radiusKm) {
			const circle = new mapkit.CircleOverlay(at(center.lat, center.lng), radiusKm * 1000, {
				style: new mapkit.Style({
					strokeColor: '#26292b',
					lineWidth: 2,
					lineDash: [6, 4],
					fillColor: '#26292b',
					fillOpacity: 0.04
				})
			});
			map.addOverlay(circle);
			items.push(circle);
		}

		placed = stores.map((s) => ({
			store: s,
			a: new mapkit.Annotation(at(s.lat, s.lng), () => pinElement(s, s.id === highlight), {
				title: s.name,
				subtitle: [s.address, `${km(s.distanceKm)} away`, !CHAINS[s.chain].online && 'No online prices']
					.filter(Boolean)
					.join(' · '),
				// Pins are anchored at their bottom centre; lift them by the pointer's height.
				anchorOffset: new DOMPoint(0, -7)
			})
		}));
		if (showCenterPin) {
			const dot = new mapkit.MarkerAnnotation(at(center.lat, center.lng), {
				color: '#26292b',
				title: 'Selected location',
				glyphText: '•',
				titleVisibility: mapkit.FeatureVisibility.Hidden
			});
			placed.push({ a: dot });
		}
		const annotations = placed.map((p) => p.a);
		map.addAnnotations(annotations);
		items.push(...annotations);
		// After MapKit has laid the pins out, so they can be measured.
		requestAnimationFrame(cull);

		// Fit the circle when there is one; MapKit's own fit of the wide pins zooms out too far.
		fitted = items[0] instanceof mapkit.CircleOverlay ? [items[0]] : items;
		// Not animated: overlapping animations (location fix + search results) can end at the wrong zoom.
		map.showItems(fitted, { animate: false, padding: padding() });
	});

	$effect(() => {
		focus;
		for (const [id, pin] of pins) focusClasses(pin, id);
		// The store the shopper points at outranks its neighbours, so it shows even when culled.
		requestAnimationFrame(cull);
	});
</script>

<div class="map" bind:this={el}>
	{#if failed}<p class="failed">The map could not be loaded: {failed}</p>{/if}
</div>

<style>
	.map {
		height: 100%;
		min-height: 280px;
		overflow: hidden;
		position: relative;
		background: #dfe4dc;
	}
	.failed {
		position: absolute;
		inset: 0;
		margin: auto;
		height: fit-content;
		max-width: 36ch;
		text-align: center;
		color: #4a4e50;
		padding: 16px;
	}
	/* Pins are created by MapKit outside this component's markup, hence :global.
	   Each pin is a small shelf label: white stock, the chain's colour as its edge. */
	:global(.store-pin) {
		position: relative;
		display: flex;
		align-items: center;
		gap: 6px;
		height: 24px;
		padding: 0 6px;
		box-sizing: border-box;
		background: #fff;
		border: 2px solid var(--chain);
		border-radius: 3px;
		box-shadow: 0 2px 4px rgb(0 0 0 / 0.3);
		cursor: pointer;
		transition:
			opacity 200ms cubic-bezier(0.16, 1, 0.3, 1),
			transform 200ms cubic-bezier(0.16, 1, 0.3, 1);
		transform-origin: bottom center;
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
	/* Each logo's own proportions (its SVG viewBox), so the image has its full width at
	   the first layout. A pin laid out before its image loads otherwise keeps a 16px box
	   with the logo spilling out of it. */
	:global(.store-pin img) {
		display: block;
		flex-shrink: 0;
		height: 11px;
	}
	:global(.store-pin img.willys) {
		aspect-ratio: 306.3 / 63.7;
	}
	:global(.store-pin img.hemkop) {
		aspect-ratio: 98 / 20;
	}
	:global(.store-pin img.lidl) {
		aspect-ratio: 1;
	}
	:global(.store-pin img.citygross) {
		aspect-ratio: 160 / 52;
	}
	:global(.store-pin img.ica) {
		aspect-ratio: 54 / 27;
	}
	:global(.store-pin img.coop) {
		aspect-ratio: 2560 / 760;
	}
	:global(.store-pin img.lidl) {
		height: 18px;
	}
	:global(.store-pin:has(img.lidl)) {
		padding: 0 1px;
	}
	:global(.store-pin img.ica) {
		height: 13px;
	}
	:global(.store-pin img.citygross) {
		height: 16px;
	}
	:global(.store-pin.hot img.citygross) {
		height: 22px;
	}
	/* Chains without online prices: shown for orientation, visually secondary. */
	:global(.store-pin.offline) {
		opacity: 0.75;
		transform: scale(0.85);
	}
	/* The cheapest store wears the yellow label and its price. */
	:global(.store-pin.hot) {
		height: 36px;
		padding: 0 10px;
		background: #ffd400;
		box-shadow: 0 4px 10px rgb(0 0 0 / 0.35);
	}
	:global(.store-pin.hot:has(img.lidl)) {
		padding: 0 10px 0 2px;
	}
	:global(.store-pin.hot img) {
		height: 15px;
	}
	:global(.store-pin.hot img.lidl) {
		height: 28px;
	}
	:global(.store-pin b) {
		font: 900 1.125rem/1 Archivo, system-ui, sans-serif;
		font-stretch: 62%;
		color: #1a1c1d;
		white-space: nowrap;
	}
	:global(.store-pin.dim) {
		opacity: 0.3;
	}
	:global(.store-pin.focus) {
		opacity: 1;
		transform: scale(1.2);
	}
	/* Hidden by culling (it would overlap a more important pin). */
	:global(.store-pin.culled) {
		visibility: hidden;
	}
	@media (prefers-reduced-motion: reduce) {
		:global(.store-pin) {
			transition: none;
		}
	}
</style>
