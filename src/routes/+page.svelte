<script lang="ts">
	import { onMount } from 'svelte';
	import StoreMap from '../lib/components/StoreMap.svelte';
	import { km, kr } from '../lib/format';
	import { distanceKm } from '../lib/geo';
	import {
		CHAINS,
		type ChainId,
		type ChainStatus,
		type Deal,
		type NearbyStore,
		type ResultRow,
		type SearchEvent,
		type Unit
	} from '../lib/types';

	const chainIds = Object.keys(CHAINS) as ChainId[];
	const pricedChains = chainIds.filter((c) => CHAINS[c].online);
	const mapOnlyChains = chainIds.filter((c) => !CHAINS[c].online);
	const unitLabel: Record<Unit, string> = { kg: 'kg', l: 'l', st: 'pc' };
	const perUnit = (n: number, unit?: Unit) => (unit ? `${kr(n)}/${unitLabel[unit]}` : kr(n));

	let query = $state('');
	let address = $state('');
	// Starts in Stockholm; the map moves it to the device location once MapKit reports one.
	let location = $state({ lat: 59.3326, lng: 18.0649, label: 'Stockholm (default)', device: false });
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
	/** Rank multi-buy deals ("5 for 55 kr") by their deal price. */
	let countDeals = $state(true);
	let hidden = $state<Set<ChainId>>(new Set());

	let controller: AbortController | undefined;

	// When some products match the query strongly, drop the ones that only share a
	// prefix with it ("fanta" vs "Fantasty" bouillon).
	const relevant = $derived(rows.some((r) => r.match === 'strong') ? rows.filter((r) => r.match === 'strong') : rows);

	// Compare unit prices only within the unit most results use (kr/kg vs kr/l vs kr/pc).
	const mainUnit = $derived.by(() => {
		const counts = new Map<Unit, number>();
		for (const r of relevant) if (r.unit) counts.set(r.unit, (counts.get(r.unit) ?? 0) + 1);
		return [...counts].sort((a, b) => b[1] - a[1])[0]?.[0];
	});

	/** Whether the ranking uses this row's multi-buy deal rather than its single-item price. */
	function usesDeal(r: ResultRow): r is ResultRow & { deal: Deal } {
		if (!countDeals || !r.deal) return false;
		if (sort === 'price') return true;
		return r.deal.unitPrice !== undefined && (r.unitPrice === undefined || r.deal.unitPrice < r.unitPrice);
	}
	const rankUnitPrice = (r: ResultRow) => (usesDeal(r) ? r.deal.unitPrice : r.unitPrice);
	const rankPrice = (r: ResultRow) => (usesDeal(r) ? r.deal.each : r.price);

	const visible = $derived(
		relevant
			.filter((r) => !hidden.has(r.chain))
			.toSorted((a, b) => {
				if (sort === 'unit') {
					const ua = a.unit === mainUnit ? (rankUnitPrice(a) ?? Infinity) : Infinity;
					const ub = b.unit === mainUnit ? (rankUnitPrice(b) ?? Infinity) : Infinity;
					if (ua !== ub) return ua - ub;
				}
				return rankPrice(a) - rankPrice(b);
			})
	);

	const cheapest = $derived(
		visible.find((r) => sort === 'price' || (r.unit === mainUnit && rankUnitPrice(r) !== undefined)) ?? visible[0]
	);

	const cheapestPerChain = $derived(
		pricedChains.map((c) => visible.find((r) => r.chain === c)).filter((r): r is ResultRow => !!r)
	);

	const storeCount = (c: ChainId) => area?.stores.filter((s) => s.chain === c).length ?? 0;

	function onLocate(p: { lat: number; lng: number }) {
		locating = false;
		if (!followDevice) return;
		// MapKit reports small GPS jitter continuously; only move for real changes.
		if (location.device && distanceKm(location, p) < 0.3) return;
		location = { ...p, label: 'Your location', device: true };
		locationError = '';
		rerun();
	}

	function onLocateError(message: string) {
		locating = false;
		if (followDevice && !location.device)
			locationError = `Couldn't get your location (${message}). Showing ${location.label}.`;
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
		loading: 'loading…',
		ok: '',
		error: 'unavailable',
		throttled: 'busy, try again shortly',
		'no-stores': 'no store nearby',
		'no-online-prices': 'no online prices'
	};

	onMount(loadArea);
</script>

{#snippet dealInfo(d: Deal, unit: Unit | undefined)}
	<div class="deal">
		<b>Deal: {d.quantity} for {kr(d.total)}</b>
		· {kr(d.each)} each{d.unitPrice !== undefined ? ` · ${perUnit(d.unitPrice, unit)}` : ''}
		<small>
			Buy {d.quantity}{d.mixAndMatch ? ' · mix & match' : ''}{d.plusDeposit ? ' · + deposit' : ''}{d.limit
				? ` · max ${d.limit} per customer`
				: ''}
		</small>
	</div>
{/snippet}

<svelte:head>
	<title>Matpris – compare grocery prices near you</title>
</svelte:head>

<main class="layout">
	<div class="top">
		<header>
			<h1>Matpris</h1>
			<p class="tag">Search for a product and compare prices at Willys, Hemköp and Lidl near you.</p>
		</header>

		<section class="controls">
			<form class="search" onsubmit={runSearch}>
				<input
					type="search"
					placeholder="Search for a product, e.g. mjölk, kaffe, bananer…"
					bind:value={query}
					aria-label="Search for a product"
				/>
				<button type="submit" disabled={searching || query.trim().length < 2}>
					{searching ? 'Searching…' : 'Search'}
				</button>
			</form>

			<div class="location">
				<span class="pin">📍 {location.label}</span>
				<button class="link" onclick={useMyPosition} disabled={locating}>Use my location</button>
				<form onsubmit={lookupAddress}>
					<input placeholder="…or enter an address or town" bind:value={address} aria-label="Address" />
					<button type="submit" disabled={locating}>Change</button>
				</form>
			</div>
			{#if locationError}<p class="error">{locationError}</p>{/if}
		</section>
	</div>

	<section class="results">
		{#if searched}
			<div class="chips">
				{#each pricedChains as c (c)}
					{@const s = status[c]}
					{@const n = relevant.filter((r) => r.chain === c).length}
					<button class="chip" class:off={hidden.has(c)} onclick={() => toggle(c)} title="Show/hide {CHAINS[c].name}">
						<img class="logo" src={CHAINS[c].logo} alt={CHAINS[c].name} />
						<small>{s?.status === 'ok' ? `${n} ${n === 1 ? 'result' : 'results'}` : statusText[s?.status ?? 'loading']}</small>
					</button>
				{/each}
				{#each mapOnlyChains as c (c)}
					<span
						class="chip static"
						title="{CHAINS[c].name} doesn't publish prices openly, so only its stores are shown on the map"
					>
						<img class="logo {c}" src={CHAINS[c].logo} alt={CHAINS[c].name} />
						<small>{storeCount(c) ? `${storeCount(c)} stores · ` : ''}no online prices</small>
					</span>
				{/each}
			</div>

			{#if searchError}<p class="error">{searchError}</p>{/if}

			{#if cheapest}
				{@const deal = usesDeal(cheapest) ? cheapest.deal : undefined}
				<div class="cheapest">
					<div class="label">Cheapest near you</div>
					<div class="best">
						{#if cheapest.image}<img src={cheapest.image} alt="" />{/if}
						<div>
							<strong>{cheapest.name}</strong>
							<div class="meta">{[cheapest.brand, cheapest.size].filter(Boolean).join(' · ')}</div>
							<div class="store">
								<img class="logo" src={CHAINS[cheapest.chain].logo} alt={CHAINS[cheapest.chain].name} />
								<b>{cheapest.store.name}</b>
								· {km(cheapest.store.distanceKm)}
							</div>
						</div>
						<div class="price big">
							{#if deal}
								{kr(deal.each)}
								<small class="when">each when you buy {deal.quantity}</small>
								{#if deal.unitPrice !== undefined}<small>{perUnit(deal.unitPrice, cheapest.unit)}</small>{/if}
							{:else}
								{kr(cheapest.price)}
								{#if cheapest.unitPrice !== undefined}<small>{perUnit(cheapest.unitPrice, cheapest.unit)}</small>{/if}
							{/if}
						</div>
					</div>
					{#if deal}{@render dealInfo(deal, cheapest.unit)}{/if}
					{#if cheapestPerChain.length > 1}
						<ul class="per-chain">
							{#each cheapestPerChain as r (r.chain)}
								<li>
									<span class="pc-logo"><img class="logo" src={CHAINS[r.chain].logo} alt={CHAINS[r.chain].name} /></span>
									<span class="pc-name">{r.name}</span>
									<b>
										{kr(rankPrice(r))}{#if usesDeal(r)}<small> each ×{r.deal.quantity}</small>{/if}
									</b>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			{/if}

			<div class="list-head">
				<span>{visible.length} products for “{searched}”</span>
				<label class="check">
					<input type="checkbox" bind:checked={countDeals} />
					Count multi-buy deals
				</label>
				<label>
					Sort by
					<select bind:value={sort}>
						<option value="unit">Unit price{mainUnit ? ` (kr/${unitLabel[mainUnit]})` : ''}</option>
						<option value="price">Price</option>
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
								{r.store.name} · {km(r.store.distanceKm)}
							</div>
							{#if r.deal}{@render dealInfo(r.deal, r.unit)}{/if}
						</div>
						<div class="price">
							{#if r.oldPrice}<span class="sale">Sale</span>{/if}
							{kr(r.price)}
							{#if r.oldPrice}<s>{kr(r.oldPrice)}</s>{/if}
							{#if r.unitPrice !== undefined}<small>{perUnit(r.unitPrice, r.unit)}</small>{/if}
						</div>
					</li>
				{:else}
					{#if !searching}<li class="empty">No results. Try another word.</li>{/if}
				{/each}
			</ul>
		{:else}
			<div class="intro">
				<p>
					Type a product above. We search the stores inside the circle on the map – it grows automatically until it
					contains enough stores. ICA and Coop don't publish their prices openly, so only their stores are shown.
				</p>
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
			<p class="area">{area.stores.length} stores within {km(area.radiusKm)}</p>
		{/if}
	</aside>

	<footer>
		Prices come straight from the chains' websites and may differ from the price in store. Willys, Hemköp and Lidl
		use one online price nationwide; the store shown is the nearest one. ICA and Coop don't publish prices openly.
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
	.chip.static {
		border-style: dashed;
		border-radius: 10px;
		cursor: default;
		background: #fafafa;
	}
	.chip .logo.ica {
		height: 14px;
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
	.deal {
		margin-top: 6px;
		background: #fff3c4;
		color: #5c4400;
		font-size: 0.85rem;
		padding: 4px 8px;
		border-radius: 6px;
		width: fit-content;
	}
	.deal small {
		display: block;
		font-size: 0.78rem;
		color: #7a5b00;
	}
	.cheapest .deal {
		margin-top: 10px;
	}
	.sale {
		font-size: 0.7rem;
		font-weight: 700;
		text-transform: uppercase;
		color: #fff;
		background: #b42318;
		padding: 1px 6px;
		border-radius: 4px;
		margin-bottom: 2px;
	}
	.price small.when {
		color: #1a7f4b;
		font-weight: 600;
	}
	.per-chain small {
		font-weight: 400;
		color: #555;
		margin-left: 3px;
	}
	.check {
		display: flex;
		align-items: center;
		gap: 4px;
		margin-left: auto;
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
