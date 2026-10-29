<script lang="ts">
	import { onMount } from 'svelte';
	import StoreMap from '../lib/components/StoreMap.svelte';
	import { distanceKm } from '../lib/geo';
	import {
		CHAINS,
		type ChainId,
		type ChainStatus,
		type NearbyStore,
		type ResultRow,
		type SearchEvent
	} from '../lib/types';

	const chainIds = Object.keys(CHAINS) as ChainId[];
	const kr = (n: number) => n.toLocaleString('sv-SE', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

	let query = $state('');
	let address = $state('');
	// Starts in Stockholm; the map moves it to the device location once MapKit reports one.
	let location = $state({ lat: 59.3326, lng: 18.0649, label: 'Stockholm (standard)', device: false });
	/** Follow device location updates until the user picks an address. */
	let followDevice = true;
	let mapRef = $state<ReturnType<typeof StoreMap>>();
	let locating = $state(false);
	let locationError = $state('');

	let area = $state<{ radiusKm: number; stores: NearbyStore[] } | null>(null);
	let rows = $state<ResultRow[]>([]);
	let status = $state<Partial<Record<ChainId, { status: ChainStatus | 'loading'; message?: string }>>>({});
	let searching = $state(false);
	let searchError = $state('');
	let searched = $state('');

	let sort = $state<'unit' | 'price'>('unit');
	let hidden = $state<Set<ChainId>>(new Set());

	let controller: AbortController | undefined;

	// When some products match the query strongly, drop the ones that only share a
	// prefix with it ("fanta" vs "Fantasty" bouillon).
	const relevant = $derived(rows.some((r) => r.match === 'strong') ? rows.filter((r) => r.match === 'strong') : rows);

	// Compare unit prices only within the unit most results use (kr/kg vs kr/l vs kr/st).
	const mainUnit = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const r of relevant) if (r.unit) counts.set(r.unit, (counts.get(r.unit) ?? 0) + 1);
		return [...counts].sort((a, b) => b[1] - a[1])[0]?.[0];
	});

	const visible = $derived(
		relevant
			.filter((r) => !hidden.has(r.chain))
			.toSorted((a, b) => {
				if (sort === 'unit') {
					const ua = a.unit === mainUnit ? (a.unitPrice ?? Infinity) : Infinity;
					const ub = b.unit === mainUnit ? (b.unitPrice ?? Infinity) : Infinity;
					if (ua !== ub) return ua - ub;
				}
				return a.price - b.price;
			})
	);

	const cheapest = $derived(
		visible.find((r) => sort === 'price' || (r.unit === mainUnit && r.unitPrice !== undefined)) ?? visible[0]
	);

	const cheapestPerChain = $derived(
		chainIds
			.map((c) => visible.find((r) => r.chain === c))
			.filter((r): r is ResultRow => !!r)
	);

	function onLocate(p: { lat: number; lng: number }) {
		locating = false;
		if (!followDevice) return;
		// MapKit reports small GPS jitter continuously; only move for real changes.
		if (location.device && distanceKm(location, p) < 0.3) return;
		location = { ...p, label: 'Din position', device: true };
		locationError = '';
		rerun();
	}

	function onLocateError(message: string) {
		locating = false;
		if (followDevice && !location.device)
			locationError = `Kunde inte hämta din position (${message}). Visar ${location.label}.`;
	}

	function useMyPosition() {
		locationError = '';
		locating = true;
		followDevice = true;
		location.device = false; // accept the next fix even if it is close by
		if (!mapRef?.locate()) {
			// The map didn't load (e.g. missing token): fall back to the browser API.
			navigator.geolocation.getCurrentPosition(
				(p) => onLocate({ lat: p.coords.latitude, lng: p.coords.longitude }),
				(e) => onLocateError(e.message),
				{ timeout: 10000 }
			);
		}
	}

	async function lookupAddress(e: SubmitEvent) {
		e.preventDefault();
		if (!address.trim()) return;
		locationError = '';
		locating = true;
		try {
			const res = await fetch(`/api/geocode?q=${encodeURIComponent(address)}`);
			if (!res.ok) throw new Error((await res.json()).message ?? res.statusText);
			const p = await res.json();
			followDevice = false;
			location = { lat: p.lat, lng: p.lng, label: p.label.split(',').slice(0, 3).join(','), device: false };
			rerun();
		} catch (err) {
			locationError = (err as Error).message;
		} finally {
			locating = false;
		}
	}

	function rerun() {
		if (searched) runSearch();
		else loadArea();
	}

	async function loadArea() {
		const res = await fetch(`/api/stores?lat=${location.lat}&lng=${location.lng}`);
		if (res.ok) area = await res.json();
	}

	async function runSearch(e?: SubmitEvent) {
		e?.preventDefault();
		const q = query.trim();
		if (q.length < 2) return;
		controller?.abort();
		controller = new AbortController();
		searching = true;
		searchError = '';
		searched = q;
		rows = [];
		status = Object.fromEntries(chainIds.map((c) => [c, { status: 'loading' }]));

		try {
			const res = await fetch(
				`/api/search?q=${encodeURIComponent(q)}&lat=${location.lat}&lng=${location.lng}`,
				{ signal: controller.signal }
			);
			if (!res.ok || !res.body) throw new Error((await res.json()).message ?? res.statusText);
			const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
			let buffer = '';
			for (;;) {
				const { value, done } = await reader.read();
				if (done) break;
				buffer += value;
				const lines = buffer.split('\n');
				buffer = lines.pop()!;
				for (const line of lines) if (line) handle(JSON.parse(line));
			}
		} catch (err) {
			if ((err as Error).name !== 'AbortError') searchError = (err as Error).message;
		} finally {
			searching = false;
		}
	}

	function handle(ev: SearchEvent | { type: 'error'; message: string }) {
		if (ev.type === 'area') area = ev;
		else if (ev.type === 'chain') {
			rows = [...rows, ...ev.results];
			status[ev.chain] = { status: ev.status, message: ev.message };
		} else if (ev.type === 'error') searchError = ev.message;
	}

	function toggle(c: ChainId) {
		const next = new Set(hidden);
		if (next.has(c)) next.delete(c);
		else next.add(c);
		hidden = next;
	}

	const statusText: Record<ChainStatus | 'loading', string> = {
		loading: 'hämtar…',
		ok: '',
		error: 'otillgänglig',
		throttled: 'upptagen, försök igen om en stund',
		'no-stores': 'ingen butik i närheten'
	};

	onMount(loadArea);
</script>

<svelte:head>
	<title>Matpris – jämför matpriser nära dig</title>
</svelte:head>

<main class="layout">
	<div class="top">
		<header>
			<h1>Matpris</h1>
			<p class="tag">Sök en vara och se priset hos Willys, Hemköp och Lidl i närheten.</p>
		</header>

		<section class="controls">
			<form class="search" onsubmit={runSearch}>
				<input
					type="search"
					placeholder="Sök vara, t.ex. mjölk, kaffe, bananer…"
					bind:value={query}
					aria-label="Sök vara"
				/>
				<button type="submit" disabled={searching || query.trim().length < 2}>
					{searching ? 'Söker…' : 'Sök'}
				</button>
			</form>

			<div class="location">
				<span class="pin">📍 {location.label}</span>
				<button class="link" onclick={useMyPosition} disabled={locating}>Använd min position</button>
				<form onsubmit={lookupAddress}>
					<input placeholder="…eller ange adress/ort" bind:value={address} aria-label="Adress" />
					<button type="submit" disabled={locating}>Byt</button>
				</form>
			</div>
			{#if locationError}<p class="error">{locationError}</p>{/if}
		</section>
	</div>

	<section class="results">
		{#if searched}
			<div class="chips">
				{#each chainIds as c (c)}
					{@const s = status[c]}
					{@const n = relevant.filter((r) => r.chain === c).length}
					<button
						class="chip"
						class:off={hidden.has(c)}
						onclick={() => toggle(c)}
						title="Visa/dölj {CHAINS[c].name}"
					>
						<img class="logo" src={CHAINS[c].logo} alt={CHAINS[c].name} />
						<small>{s?.status === 'ok' ? `${n} träffar` : statusText[s?.status ?? 'loading']}</small>
					</button>
				{/each}
				<span class="chip muted" title="Stöds inte ännu – se README">ICA, Coop: ej med</span>
			</div>

			{#if searchError}<p class="error">{searchError}</p>{/if}

			{#if cheapest}
				<div class="cheapest">
					<div class="label">Billigast nära dig</div>
					<div class="best">
						{#if cheapest.image}<img src={cheapest.image} alt="" />{/if}
						<div>
							<strong>{cheapest.name}</strong>
							<div class="meta">{[cheapest.brand, cheapest.size].filter(Boolean).join(' · ')}</div>
							<div class="store">
								<img class="logo" src={CHAINS[cheapest.chain].logo} alt={CHAINS[cheapest.chain].name} />
								<b>{cheapest.store.name}</b>
								· {cheapest.store.distanceKm.toLocaleString('sv-SE', { maximumFractionDigits: 1 })} km
							</div>
						</div>
						<div class="price big">
							{kr(cheapest.price)} kr
							{#if cheapest.unitPrice}<small>{kr(cheapest.unitPrice)} kr/{cheapest.unit}</small>{/if}
						</div>
					</div>
					{#if cheapestPerChain.length > 1}
						<ul class="per-chain">
							{#each cheapestPerChain as r (r.chain)}
								<li>
									<span class="pc-logo"><img class="logo" src={CHAINS[r.chain].logo} alt={CHAINS[r.chain].name} /></span>
									<span class="pc-name">{r.name}</span>
									<b>{kr(r.price)} kr</b>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			{/if}

			<div class="list-head">
				<span>{visible.length} varor för ”{searched}”</span>
				<label>
					Sortera
					<select bind:value={sort}>
						<option value="unit">Jämförpris{mainUnit ? ` (kr/${mainUnit})` : ''}</option>
						<option value="price">Pris</option>
					</select>
				</label>
			</div>

			<ul class="list">
				{#each visible as r (r.chain + r.id)}
					<li>
						{#if r.image}<img src={r.image} alt="" loading="lazy" />{:else}<div class="noimg"></div>{/if}
						<div class="info">
							<a href={r.url} target="_blank" rel="noopener">{r.name}</a>
							<div class="meta">{[r.brand, r.size].filter(Boolean).join(' · ')}</div>
							<div class="store">
								<img class="logo" src={CHAINS[r.chain].logo} alt={CHAINS[r.chain].name} />
								{r.store.name} · {r.store.distanceKm.toLocaleString('sv-SE', { maximumFractionDigits: 1 })} km
							</div>
							{#if r.promo}<div class="promo">{r.promo}</div>{/if}
						</div>
						<div class="price">
							{kr(r.price)} kr
							{#if r.oldPrice}<s>{kr(r.oldPrice)}</s>{/if}
							{#if r.unitPrice}<small>{kr(r.unitPrice)} kr/{r.unit}</small>{/if}
						</div>
					</li>
				{:else}
					{#if !searching}<li class="empty">Inga träffar. Prova ett annat ord.</li>{/if}
				{/each}
			</ul>
		{:else}
			<div class="intro">
				<p>Skriv en vara ovan. Vi letar i butikerna inom cirkeln på kartan – den växer automatiskt tills den
					innehåller tillräckligt många butiker.</p>
			</div>
		{/if}
	</section>

	<aside class="mapcol">
		<StoreMap
			bind:this={mapRef}
			center={location}
			showCenterPin={!location.device}
			onlocate={onLocate}
			onlocateerror={onLocateError}
			radiusKm={area?.radiusKm}
			stores={area?.stores ?? []}
			highlight={cheapest?.store.id}
		/>
		{#if area}
			<p class="area">
				{area.stores.length} butiker inom {area.radiusKm.toLocaleString('sv-SE')} km
			</p>
		{/if}
	</aside>

	<footer>
		Priser hämtas direkt från kedjornas webbplatser och kan skilja sig från priset i butik. Willys, Hemköp och
		Lidl har samma pris i hela landet på nätet; butiken som visas är den närmaste.
	</footer>
</main>

<style>
	:global(body) {
		margin: 0;
		font-family: system-ui, -apple-system, 'Segoe UI', sans-serif;
		background: #f6f7f5;
		color: #1b1f1d;
	}
	/* Desktop: top bar and results on the left, a full-height map on the right.
	   Mobile: top bar, then the map, then the results. */
	.layout {
		max-width: 1480px;
		margin: 0 auto;
		padding: 16px;
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1.15fr);
		grid-template-rows: auto 1fr auto;
		grid-template-areas: 'top map' 'results map' 'footer map';
		column-gap: 16px;
		align-items: start;
	}
	.top {
		grid-area: top;
	}
	.results {
		grid-area: results;
		margin-top: 16px;
	}
	footer {
		grid-area: footer;
	}
	header h1 {
		margin: 8px 0 0;
		font-size: 2rem;
		color: #1a7f4b;
	}
	.tag {
		margin: 4px 0 16px;
		color: #555;
	}
	.controls {
		background: #fff;
		border-radius: 12px;
		padding: 16px;
		box-shadow: 0 1px 3px #0001;
	}
	.search {
		display: flex;
		gap: 8px;
	}
	.search input {
		flex: 1;
		font-size: 1.1rem;
		padding: 12px 14px;
		border: 2px solid #d5dad7;
		border-radius: 10px;
		min-width: 0;
	}
	.search input:focus {
		outline: none;
		border-color: #1a7f4b;
	}
	button {
		font: inherit;
		cursor: pointer;
		border: none;
		border-radius: 10px;
		padding: 10px 18px;
		background: #1a7f4b;
		color: #fff;
		font-weight: 600;
	}
	button:disabled {
		opacity: 0.5;
		cursor: default;
	}
	button.link {
		background: none;
		color: #1a7f4b;
		padding: 4px 6px;
		text-decoration: underline;
	}
	.location {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin-top: 12px;
		font-size: 0.95rem;
	}
	.location form {
		display: flex;
		gap: 6px;
		margin-left: auto;
	}
	.location input {
		padding: 8px 10px;
		border: 1px solid #d5dad7;
		border-radius: 8px;
		min-width: 0;
	}
	.location form button {
		padding: 8px 12px;
	}
	.pin {
		font-weight: 600;
	}
	.error {
		color: #b42318;
	}
	.mapcol {
		grid-area: map;
		position: sticky;
		top: 16px;
		height: calc(100vh - 32px);
		min-height: 480px;
		display: flex;
		flex-direction: column;
	}
	.mapcol > :global(.map) {
		flex: 1;
	}
	.area {
		text-align: center;
		color: #555;
		font-size: 0.9rem;
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-bottom: 12px;
	}
	.chip {
		background: #fff;
		color: #1b1f1d;
		border: 1px solid #d5dad7;
		padding: 6px 12px;
		font-weight: 500;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.chip small {
		color: #666;
		font-weight: 400;
	}
	.logo {
		height: 15px;
		width: auto;
		vertical-align: middle;
	}
	.chip .logo {
		height: 17px;
	}
	.chip.off {
		opacity: 0.45;
	}
	.chip.muted {
		color: #888;
		border-style: dashed;
		font-size: 0.85rem;
		padding: 6px 12px;
		border-radius: 10px;
	}

	.cheapest {
		background: #e8f5ee;
		border: 2px solid #1a7f4b;
		border-radius: 12px;
		padding: 14px;
		margin-bottom: 16px;
	}
	.cheapest .label {
		font-size: 0.8rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: #1a7f4b;
		font-weight: 700;
		margin-bottom: 8px;
	}
	.best {
		display: flex;
		gap: 12px;
		align-items: center;
	}
	.best > img {
		width: 64px;
		height: 64px;
		object-fit: contain;
		background: #fff;
		border-radius: 8px;
	}
	.best > div:nth-child(2) {
		flex: 1;
		min-width: 0;
	}
	.per-chain {
		list-style: none;
		padding: 10px 0 0;
		margin: 10px 0 0;
		border-top: 1px solid #1a7f4b33;
		display: grid;
		gap: 4px;
		font-size: 0.9rem;
	}
	.per-chain li {
		display: flex;
		gap: 8px;
	}
	.pc-logo {
		width: 72px;
		flex-shrink: 0;
	}
	.pc-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: #444;
	}
	.list-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 8px;
		color: #555;
		font-size: 0.9rem;
		gap: 8px;
	}
	select {
		font: inherit;
		padding: 4px 6px;
		border-radius: 6px;
	}
	.list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: grid;
		gap: 8px;
	}
	.list li {
		display: flex;
		gap: 12px;
		background: #fff;
		border-radius: 10px;
		padding: 10px;
		box-shadow: 0 1px 2px #0001;
		align-items: center;
	}
	.list li.empty {
		justify-content: center;
		color: #666;
		padding: 24px;
	}
	.list img,
	.noimg {
		width: 56px;
		height: 56px;
		object-fit: contain;
		flex-shrink: 0;
	}
	.info {
		flex: 1;
		min-width: 0;
	}
	.info a {
		color: inherit;
		font-weight: 600;
		text-decoration: none;
	}
	.info a:hover {
		text-decoration: underline;
	}
	.meta {
		color: #666;
		font-size: 0.85rem;
	}
	.store {
		font-size: 0.85rem;
		margin-top: 4px;
		color: #333;
	}
	.store .logo {
		margin-right: 4px;
	}
	.promo {
		display: inline-block;
		margin-top: 4px;
		background: #fff3c4;
		color: #7a5b00;
		font-size: 0.8rem;
		padding: 2px 6px;
		border-radius: 4px;
	}
	.price {
		text-align: right;
		font-weight: 700;
		font-size: 1.1rem;
		white-space: nowrap;
		display: flex;
		flex-direction: column;
		align-items: flex-end;
	}
	.price.big {
		font-size: 1.5rem;
	}
	.price s {
		font-weight: 400;
		font-size: 0.85rem;
		color: #b42318;
	}
	.price small {
		font-weight: 400;
		font-size: 0.8rem;
		color: #666;
	}
	.intro {
		background: #fff;
		border-radius: 12px;
		padding: 20px;
		color: #444;
	}
	footer {
		margin: 32px 0 8px;
		font-size: 0.8rem;
		color: #777;
	}
	@media (max-width: 860px) {
		.layout {
			grid-template-columns: minmax(0, 1fr);
			grid-template-rows: none;
			grid-template-areas: 'top' 'map' 'results' 'footer';
		}
		.mapcol {
			position: static;
			height: 65vh;
			min-height: 360px;
			margin-top: 16px;
		}
		.location form {
			margin-left: 0;
			width: 100%;
		}
		.location form input {
			flex: 1;
		}
	}
</style>
