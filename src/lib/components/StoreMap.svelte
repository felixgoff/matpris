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
	}
	let { center, radiusKm, stores, highlight }: Props = $props();

	let el: HTMLDivElement;
	let map = $state<mapkit.Map>();
	let failed = $state('');

	onMount(() => {
		let cancelled = false;
		let created: mapkit.Map | undefined;
		loadMapKit().then(
			(mk) => {
				if (cancelled) return;
				created = new mk.Map(el, {
					showsPointsOfInterest: false,
					showsMapTypeControl: false,
					isRotationEnabled: false
				});
				map = created;
			},
			(e: Error) => (failed = e.message)
		);
		return () => {
			cancelled = true;
			created?.destroy();
		};
	});

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
		// stores always collide, so every marker is required and names only show
		// in the callout when a marker is tapped.
		const pin = {
			displayPriority: mapkit.Annotation.DisplayPriority.Required,
			titleVisibility: mapkit.FeatureVisibility.Hidden,
			subtitleVisibility: mapkit.FeatureVisibility.Hidden
		};
		const annotations = stores.map((s) => {
			const hot = s.id === highlight;
			return new mapkit.MarkerAnnotation(at(s.lat, s.lng), {
				...pin,
				color: CHAINS[s.chain].color,
				title: s.name,
				subtitle: `${s.address} · ${s.distanceKm.toLocaleString('sv-SE', { maximumFractionDigits: 1 })} km bort`,
				glyphText: hot ? '★' : CHAINS[s.chain].name[0],
				selected: hot
			});
		});
		annotations.push(
			new mapkit.MarkerAnnotation(at(center.lat, center.lng), {
				...pin,
				color: '#1b1f1d',
				title: 'Du är här',
				glyphText: '•'
			})
		);
		map.addAnnotations(annotations);
		items.push(...annotations);

		map.showItems(items, { animate: true, padding: new mapkit.Padding(40, 30, 30, 30) });
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
</style>
