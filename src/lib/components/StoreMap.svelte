<script lang="ts">
	import 'leaflet/dist/leaflet.css';
	import type { Circle, LayerGroup, Map as LeafletMap } from 'leaflet';
	import { onMount } from 'svelte';
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
	let L: typeof import('leaflet') | undefined = $state();
	let map: LeafletMap;
	let layer: LayerGroup;
	let circle: Circle | undefined;

	onMount(() => {
		let cancelled = false;
		// Leaflet touches `window`, so load it in the browser only.
		import('leaflet').then((mod) => {
			if (cancelled) return;
			L = mod.default ?? mod;
			map = L.map(el, { scrollWheelZoom: false }).setView([center.lat, center.lng], 12);
			L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
				maxZoom: 19,
				attribution: '&copy; OpenStreetMap'
			}).addTo(map);
			layer = L.layerGroup().addTo(map);
		});
		return () => {
			cancelled = true;
			map?.remove();
		};
	});

	$effect(() => {
		if (!L) return;
		layer.clearLayers();
		circle = undefined;
		L.circleMarker([center.lat, center.lng], { radius: 6, color: '#111', fillOpacity: 1 })
			.bindTooltip('Du är här')
			.addTo(layer);
		if (radiusKm) {
			circle = L.circle([center.lat, center.lng], {
				radius: radiusKm * 1000,
				color: '#1a7f4b',
				weight: 1.5,
				fillOpacity: 0.05,
				dashArray: '6 4'
			}).addTo(layer);
		}
		for (const s of stores) {
			const hot = s.id === highlight;
			L.circleMarker([s.lat, s.lng], {
				radius: hot ? 11 : 7,
				color: hot ? '#1a7f4b' : '#fff',
				weight: hot ? 4 : 2,
				fillColor: CHAINS[s.chain].color,
				fillOpacity: 1
			})
				.bindPopup(`<b>${s.name}</b><br>${s.address}<br>${s.distanceKm.toLocaleString('sv-SE', { maximumFractionDigits: 1 })} km bort`)
				.addTo(layer);
		}
		if (circle) map.fitBounds(circle.getBounds(), { padding: [10, 10] });
		else map.setView([center.lat, center.lng], 12);
	});
</script>

<div class="map" bind:this={el}></div>

<style>
	.map {
		height: 100%;
		min-height: 320px;
		border-radius: 12px;
		overflow: hidden;
		z-index: 0;
	}
</style>
