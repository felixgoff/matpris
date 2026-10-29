<script lang="ts">
	import { onMount } from 'svelte';
	import StoreMap from '../lib/components/StoreMap.svelte';
	import { km, kr, krParts } from '../lib/format';
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
	// en-GB: "A, B and C", without the serial comma, like the rest of the copy.
	const listNames = (names: string[]) => new Intl.ListFormat('en-GB').format(names);
	const unitLabel: Record<Unit, string> = { kg: 'kg', l: 'l', st: 'pc' };
	const perUnit = (n: number, unit?: Unit) => (unit ? `${kr(n)}/${unitLabel[unit]}` : kr(n));

	let query = $state('');
	let address = $state('');
	// Starts in Stockholm; the map moves it to the device location once MapKit reports one.
	let location = $state({ lat: 55.70584, lng: 13.19321, label: 'Lund (default)', device: false });
	/** Follow device location updates until the user picks an address. */
	let followDevice = true;
	let mapRef = $state<ReturnType<typeof StoreMap>>();
	let locating = $state(false);
	let locationError = $state('');
	/** The address form is folded away until the shopper asks to change location. */
	let editingAddress = $state(false);
	/** Store under the pointer or keyboard focus in the list; the map lifts its pin. */
	let focusStore = $state<string>();
	const suggestions = ['mjölk', 'kaffe', 'bananer', 'ägg', 'smör'];
	/** Only a mouse hovers; a touch would leave the map dimmed after the finger lifts. */
	const point = (e: PointerEvent, id?: string) => {
		if (e.pointerType === 'mouse') focusStore = id;
	};

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

	// Rank only the products that are the thing searched for ("Banan Klass 1"). Things it
	// merely flavours ("Banan & Vanilj Yoghurt") and prefix matches ("Fantasty" for
	// "fanta") fold into a second list, so they can't win "Cheapest near you".
	const tier = $derived((['best', 'mention', 'weak'] as const).find((t) => rows.some((r) => r.match === t)));
	const relevant = $derived(rows.filter((r) => r.match === tier));

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
	/** Price of one item; for loose fruit, one piece at its average weight. */
	const rankPrice = (r: ResultRow) => (usesDeal(r) ? r.deal.each : (r.approxEach ?? r.price));
	/** Sold by weight with no known piece weight: its price is per kg, not per item. */
	const loose = (r: ResultRow) => !!r.perKg && r.approxEach === undefined;
	const comparable = (r: ResultRow) =>
		sort === 'unit' ? r.unit === mainUnit && rankUnitPrice(r) !== undefined : !loose(r);

	// Comparable rows first, by the chosen basis; the rest after them, by item price.
	const byRank = (a: ResultRow, b: ResultRow) => {
		const ca = comparable(a),
			cb = comparable(b);
		if (ca !== cb) return ca ? -1 : 1;
		if (ca && sort === 'unit') {
			const d = rankUnitPrice(a)! - rankUnitPrice(b)!;
			if (d) return d;
		}
		return rankPrice(a) - rankPrice(b);
	};

	const visible = $derived(relevant.filter((r) => !hidden.has(r.chain)).toSorted(byRank));
	const others = $derived(rows.filter((r) => r.match !== tier && !hidden.has(r.chain)).toSorted(byRank));
	/** Index of the first row that can't be compared on the chosen basis, for the divider. */
	const firstOther = $derived(visible.findIndex((r) => !comparable(r)));

	/** The cheapest store's pin repeats the answer's large figure. */
	const pinLabel = $derived.by(() => {
		if (!cheapest) return undefined;
		const f = figures(cheapest);
		return `${f.approx ? '≈ ' : ''}${kr(f.big)}${f.per}`;
	});

	/** A label shows the figure the list is ranked by in large type, and the other price under it. */
	function figures(r: ResultRow) {
		const unitPrice = rankUnitPrice(r);
		const each = usesDeal(r)
			? `${kr(r.deal.each)} each when you buy ${r.deal.quantity}`
			: loose(r)
				? `${kr(r.price)}/kg`
				: r.approxEach !== undefined
					? `≈ ${kr(r.approxEach)} each`
					: kr(r.price);
		if (sort === 'unit' && comparable(r))
			return { big: unitPrice!, per: `/${unitLabel[r.unit!]}`, approx: false, small: each };
		return {
			big: loose(r) ? r.price : rankPrice(r),
			per: loose(r) ? '/kg' : '',
			approx: !loose(r) && !usesDeal(r) && r.approxEach !== undefined,
			small: unitPrice !== undefined ? perUnit(unitPrice, r.unit) : undefined,
			note: usesDeal(r) ? `each when you buy ${r.deal.quantity}` : undefined
		};
	}

	const cheapest = $derived(
		visible.find(comparable) ?? visible[0]
	);

	const cheapestPerChain = $derived(
		pricedChains.map((c) => visible.find((r) => r.chain === c)).filter((r): r is ResultRow => !!r)
	);

	/** "Willys Hornstull" → "Hornstull": the logo beside it already names the chain. */
	const storeName = (s: NearbyStore) => s.name.replace(new RegExp(`^${CHAINS[s.chain].name}\\s+`, 'i'), '');

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
		if (followDevice && !location.device) {
			const why = /denied/i.test(message) ? 'Location access is off' : "We couldn't find your location";
			locationError = `${why}, so this shows ${location.label}. Use “Change address” to search somewhere else.`;
		}
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
			editingAddress = false;
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

	function suggest(q: string) {
		query = q;
		runSearch();
	}

	function toggle(c: ChainId) {
		const next = new Set(hidden);
		if (next.has(c)) next.delete(c);
		else next.add(c);
		hidden = next;
	}

	const statusText: Record<ChainStatus | 'loading', string> = {
		loading: 'searching…',
		ok: '',
		error: 'unavailable',
		throttled: 'busy, try again',
		'no-stores': 'no store nearby',
		'no-online-prices': 'no online prices'
	};

	onMount(loadArea);
</script>

{#snippet price(n: number)}
	{@const [whole, ore] = krParts(n)}
	<span class="num" aria-hidden="true">{whole}<sup>{ore}</sup></span><span class="sr">{kr(n)}</span>
{/snippet}

{#snippet dealInfo(d: Deal, unit: Unit | undefined)}
	<div class="deal">
		<span class="deal-tag">{d.quantity} for {kr(d.total)}</span>
		<span>{kr(d.each)} each{d.unitPrice !== undefined ? ` · ${perUnit(d.unitPrice, unit)}` : ''}</span>
		<small>
			Buy {d.quantity}{d.mixAndMatch ? ' · mix & match' : ''}{d.plusDeposit ? ' · + deposit' : ''}{d.limit
				? ` · max ${d.limit} per customer`
				: ''}
		</small>
	</div>
{/snippet}

{#snippet figure(r: ResultRow)}
	{@const f = figures(r)}
	<div class="price">
		{#if r.oldPrice}<span class="sale">Sale</span>{/if}
		<span class="figure"
			>{#if f.approx}<span class="approx" aria-hidden="true">≈</span>{/if}{@render price(f.big)}{#if f.per}<span
					class="per">{f.per}</span
				>{/if}</span
		>
		{#if f.small}<p class="unit">{f.small}</p>{/if}
		{#if r.oldPrice}<s>{kr(r.oldPrice)}</s>{/if}
		{#if f.note}<p class="when">{f.note}</p>{/if}
	</div>
{/snippet}

{#snippet thumb(r: ResultRow)}
	{#if r.image}<img class="thumb" src={r.image} alt="" loading="lazy" />{:else}<span class="thumb"></span>{/if}
{/snippet}

{#snippet shelfLabel(r: ResultRow)}
	<li
		class="label"
		onpointerenter={(e) => point(e, r.store.id)}
		onpointerleave={(e) => point(e)}
		onfocusin={() => (focusStore = r.store.id)}
		onfocusout={() => (focusStore = undefined)}
	>
		{@render thumb(r)}
		<div class="info">
			<a class="name" href={r.url} target="_blank" rel="noopener">{r.name}</a>
			<p class="meta">{[r.brand, r.size].filter(Boolean).join(' · ')}</p>
			<p class="store">{@render logo(r.chain)}{storeName(r.store)} · {km(r.store.distanceKm)}</p>
			{#if r.deal}{@render dealInfo(r.deal, r.unit)}{/if}
		</div>
		{@render figure(r)}
	</li>
{/snippet}

{#snippet logo(c: ChainId)}
	<img class="logo {c}" src={CHAINS[c].logo} alt={CHAINS[c].name} />
{/snippet}

<svelte:head>
	<title>Matpris – compare grocery prices near you</title>
</svelte:head>

<div class="app">
	<header class="rail">
		<h1 class="mark">Mat<span>pris</span></h1>
		<form class="search" role="search" onsubmit={runSearch}>
			<input
				type="search"
				placeholder="e.g. mjölk, kaffe, bananer"
				bind:value={query}
				aria-label="Search for a product"
				enterkeyhint="search"
			/>
			<button type="submit" disabled={searching || query.trim().length < 2}>
				{searching ? 'Searching…' : 'Search'}
			</button>
		</form>
		<div class="where">
			<span class="near">Near <b>{location.label}</b></span>
			<button class="link" onclick={useMyPosition} disabled={locating}>
				<svg viewBox="0 0 16 16" aria-hidden="true"
					><path d="M14 2 2 7.2l5.3 1.5L8.8 14z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" /></svg
				>
				{locating && !editingAddress ? 'Locating…' : 'Use my location'}
			</button>
			<button
				class="link"
				onclick={() => (editingAddress = !editingAddress)}
				aria-expanded={editingAddress}
				aria-controls="address"
			>
				Change address
			</button>
		</div>
	</header>

	{#if editingAddress}
		<form id="address" class="address" onsubmit={lookupAddress}>
			<label for="address-input">Search near an address or town</label>
			<div class="address-row">
				<input
					id="address-input"
					bind:value={address}
					placeholder="e.g. Storgatan 1, Umeå"
					autocomplete="street-address"
					{@attach (el) => el.focus()}
				/>
				<button type="submit" disabled={locating || !address.trim()}>{locating ? 'Finding…' : 'Use this address'}</button>
				<button type="button" class="link" onclick={() => (editingAddress = false)}>Cancel</button>
			</div>
		</form>
	{/if}

	<main class="layout">
		<div class="col">
			<div class="summary">
				{#if locationError}<p class="notice" role="alert">{locationError}</p>{/if}

				{#if searched}
					<div class="share" role="group" aria-label="Show or hide chains">
						{#each pricedChains as c (c)}
							{@const s = status[c]?.status ?? 'loading'}
							{@const n = relevant.filter((r) => r.chain === c).length}
							<button
								class="seg"
								class:down={s === 'error' || s === 'throttled'}
								style:flex-grow={s === 'ok' ? Math.max(n, 1) : 1}
								style:--chain={CHAINS[c].color}
								aria-pressed={!hidden.has(c)}
								title="{hidden.has(c) ? 'Show' : 'Hide'} {CHAINS[c].name}{status[c]?.message
									? ` (${status[c]?.message})`
									: s === 'ok' && !n
										? ' (its website lists nothing matching with a price; the store may still sell it)'
										: ''}"
								onclick={() => toggle(c)}
							>
								{@render logo(c)}
								{#if s === 'ok' && n}
									<b class="num">{n}</b>
								{:else}
									<span>{s === 'ok' ? 'not listed' : statusText[s]}</span>
								{/if}
							</button>
						{/each}
						{#if mapOnlyChains.length}
							<span
								class="seg none"
								title="{listNames(
									mapOnlyChains.map((c) => `${CHAINS[c].name} (${storeCount(c)} stores)`)
								)} don't publish prices openly, so only their stores are shown on the map"
							>
								{#each mapOnlyChains as c (c)}{@render logo(c)}{/each}
								<span>no online prices</span>
							</span>
						{/if}
					</div>
					<p class="count" aria-live="polite">
						{searching && !rows.length
							? `Searching ${pricedChains.length} chains for “${searched}”…`
							: `${visible.length} ${visible.length === 1 ? 'product' : 'products'} for “${searched}”`}
						·
						{hidden.size
							? `${[...hidden].map((c) => CHAINS[c].name).join(', ')} hidden, tap to show`
							: 'Tap a chain to hide it'}
					</p>

					<div class="ranking">
						<fieldset class="sort">
							<legend>Rank by</legend>
							<label
								><input type="radio" bind:group={sort} value="unit" />{mainUnit
									? `Price per ${unitLabel[mainUnit]}`
									: 'Unit price'}</label
							>
							<label><input type="radio" bind:group={sort} value="price" />Item price</label>
						</fieldset>
						<label class="check"><input type="checkbox" bind:checked={countDeals} /> Count multi-buy deals</label>
					</div>

					{#if searchError}<p class="notice" role="alert">{searchError}</p>{/if}

					{#if cheapest}
						{@const deal = usesDeal(cheapest) ? cheapest.deal : undefined}
						<h2 class="answer-head">Cheapest near you</h2>
						<p class="basis">
							Lowest {sort === 'unit' && mainUnit ? `price per ${unitLabel[mainUnit]}` : 'item price'}{countDeals
								? ', multi-buy deals counted'
								: ', single items only'}
						</p>
						{#key cheapest.chain + cheapest.id}
							<article
								class="label answer"
								onpointerenter={(e) => point(e, cheapest.store.id)}
								onpointerleave={(e) => point(e)}
							>
								{@render thumb(cheapest)}
								<div class="info">
									<a class="name" href={cheapest.url} target="_blank" rel="noopener">{cheapest.name}</a>
									<p class="meta">{[cheapest.brand, cheapest.size].filter(Boolean).join(' · ')}</p>
									<p class="store">{@render logo(cheapest.chain)}{storeName(cheapest.store)} · {km(cheapest.store.distanceKm)}</p>
								</div>
								{@render figure(cheapest)}
								{#if deal}<div class="wide">{@render dealInfo(deal, cheapest.unit)}</div>{/if}
								{#if cheapestPerChain.length > 1}
									<div class="best wide">
										<h3>Best at each chain</h3>
										<ul>
											{#each cheapestPerChain as r (r.chain)}
												{@const f = figures(r)}
												<li>
													<span class="best-logo">{@render logo(r.chain)}</span>
													<span class="best-name">{r.name}</span>
													{#if f.small}<small>{f.small}</small>{/if}
													<b>{f.approx ? '≈ ' : ''}{kr(f.big)}{f.per}</b>
												</li>
											{/each}
										</ul>
									</div>
								{/if}
							</article>
						{/key}
					{/if}
				{:else}
					<div class="intro">
						<h2>What's cheapest near you?</h2>
						<p>
							Search for a grocery product and compare live prices at the {listNames(pricedChains.map((c) => CHAINS[c].name))}
							stores around you, ranked by price per kilo or litre.
						</p>
						<div class="try">
							<span>Try</span>
							{#each suggestions as s (s)}<button class="tag" onclick={() => suggest(s)}>{s}</button>{/each}
						</div>
						<p class="fine">
							We search the stores inside the circle on the map. It grows automatically until it contains enough
							stores.{#if mapOnlyChains.length}
								{listNames(mapOnlyChains.map((c) => CHAINS[c].name))} don't publish their prices openly, so only their stores
								are shown.{/if}
						</p>
					</div>
				{/if}
			</div>

			{#if searched}
				<section class="results" aria-labelledby="all-results">
					<h2 id="all-results" class="list-head">All results</h2>

					<ul class="shelves">
						{#each visible as r, i (r.chain + r.id)}
							{#if i === firstOther && i > 0}
								<li class="divider">
									{sort === 'unit' && mainUnit
										? `Can't be compared per ${unitLabel[mainUnit]}`
										: 'Sold by weight, priced per kg'}
								</li>
							{/if}
							{@render shelfLabel(r)}
						{:else}
							{#if !searching}
								<li class="label empty">No products matched “{searched}”. Try a shorter or different word.</li>
							{/if}
						{/each}
					</ul>

					{#if others.length}
						<details class="others">
							<summary>
								{others.length} other {others.length === 1 ? 'product mentions' : 'products mention'} “{searched}”
								<small>flavours, ingredients, baby food</small>
							</summary>
							<ul class="shelves">
								{#each others as r (r.chain + r.id)}{@render shelfLabel(r)}{/each}
							</ul>
						</details>
					{/if}
				</section>
			{/if}

			<footer>
				Prices come straight from the chains' websites and may differ from the price in store. Where a chain has one online
				price nationwide, the store shown is the nearest one.{#if mapOnlyChains.length}
					{listNames(mapOnlyChains.map((c) => CHAINS[c].name))} don't publish prices openly.{/if}
			</footer>
		</div>

		<aside class="mapcol" aria-label="Map of stores">
			<StoreMap
				bind:this={mapRef}
				center={location}
				showCenterPin={!location.device}
				onlocate={onLocate}
				onlocateerror={onLocateError}
				radiusKm={area?.radiusKm}
				stores={area?.stores ?? []}
				highlight={cheapest?.store.id}
				highlightLabel={pinLabel}
				focus={focusStore}
			/>
			{#if area}<p class="area">{area.stores.length} stores within {km(area.radiusKm)}</p>{/if}
		</aside>
	</main>
</div>

<style>
	:global(:root) {
		--steel: #26292b;
		--steel-edge: #3a3e41;
		--ground: #e6e8e4;
		--label: #ffffff;
		--yellow: #ffd400;
		--yellow-ink: #4b4000;
		--offer: #d4001a;
		--offer-ink: #a30015;
		--ink: #1a1c1d;
		--ink-2: #4a4e50;
		--rule: #9a9e9c;
		--hang: cubic-bezier(0.16, 1, 0.3, 1);
		color-scheme: light;
		accent-color: var(--steel);
	}
	:global(body) {
		margin: 0;
		font-family: Archivo, system-ui, -apple-system, 'Segoe UI', sans-serif;
		background: var(--ground);
		color: var(--ink);
		caret-color: var(--steel);
		-webkit-font-smoothing: antialiased;
	}
	:global(::selection) {
		background: var(--steel);
		color: #fff;
	}
	:global(:focus-visible) {
		outline: 3px solid var(--steel);
		outline-offset: 2px;
	}
	p,
	h1,
	h2,
	h3 {
		margin: 0;
	}
	button {
		font: inherit;
		cursor: pointer;
	}
	button:disabled {
		cursor: default;
	}
	.sr,
	.sort legend {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}

	/* The shelf rail: wordmark, search and location on charcoal steel. */
	.rail {
		background: var(--steel);
		color: #fff;
		display: flex;
		align-items: center;
		gap: 12px 24px;
		padding: 13px 28px;
		border-bottom: 4px solid var(--steel-edge);
	}
	.rail :focus-visible,
	.address :focus-visible {
		outline-color: #fff;
	}
	.mark {
		font-size: 1.75rem;
		font-weight: 900;
		font-stretch: 70%;
		text-transform: uppercase;
		letter-spacing: -0.01em;
		line-height: 1;
	}
	.mark span {
		color: var(--yellow);
	}
	.search {
		flex: 1;
		max-width: 640px;
		display: flex;
		gap: 3px;
	}
	.search input,
	.address input {
		flex: 1;
		min-width: 0;
		height: 46px;
		border: 0;
		border-radius: 3px;
		padding: 0 16px;
		font: 600 1.0625rem Archivo, system-ui, sans-serif;
		color: var(--ink);
		background: var(--label);
	}
	.search input::placeholder,
	.address input::placeholder {
		color: #6a6e70;
		font-weight: 400;
	}
	.search button,
	.address button[type='submit'] {
		height: 46px;
		padding: 0 22px;
		border: 0;
		border-radius: 3px;
		background: var(--label);
		color: var(--ink);
		font-weight: 800;
		font-stretch: 80%;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.search button:hover:not(:disabled),
	.address button[type='submit']:hover:not(:disabled) {
		background: var(--ground);
	}
	.search button:disabled,
	.address button:disabled {
		background: #8d9193;
		color: #26292b;
	}
	.where {
		margin-left: auto;
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 4px 18px;
		font-size: 0.875rem;
		color: #c9ccca;
		min-width: 0;
	}
	.near {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 26ch;
	}
	.near b {
		color: #fff;
	}
	.link {
		background: none;
		border: 0;
		padding: 4px 0;
		color: #fff;
		font-weight: 600;
		text-decoration: underline;
		text-underline-offset: 3px;
		text-decoration-thickness: 1px;
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.link:hover:not(:disabled) {
		text-decoration-thickness: 2px;
	}
	.link:disabled {
		color: #c9ccca;
	}
	.link svg {
		width: 14px;
		height: 14px;
	}
	.address {
		background: #34383b;
		color: #fff;
		padding: 12px 28px 16px;
	}
	.address label {
		display: block;
		font-size: 0.8125rem;
		color: #c9ccca;
		margin-bottom: 6px;
	}
	.address-row {
		display: flex;
		align-items: center;
		gap: 10px 16px;
		max-width: 720px;
	}
	.address input,
	.address button[type='submit'] {
		height: 40px;
	}

	/* Desktop: a fixed app shell. The left column scrolls; the map fills the rest. */
	.app {
		display: flex;
		flex-direction: column;
		height: 100dvh;
	}
	.layout {
		flex: 1;
		min-height: 0;
		display: grid;
		grid-template-columns: minmax(0, 680px) minmax(0, 1fr);
	}
	.col {
		overflow-y: auto;
		padding: 22px 28px 0;
		scrollbar-width: thin;
		scrollbar-color: var(--rule) transparent;
	}
	.mapcol {
		position: relative;
		display: flex;
		border-left: 4px solid var(--steel-edge);
	}
	.mapcol > :global(.map) {
		flex: 1;
	}
	.area {
		position: absolute;
		top: 12px;
		left: 12px;
		background: var(--label);
		padding: 7px 11px;
		border-radius: 3px;
		font-size: 0.8125rem;
		font-weight: 700;
		box-shadow: 0 2px 6px rgb(0 0 0 / 0.18);
	}

	.notice {
		color: var(--offer-ink);
		font-weight: 600;
		font-size: 0.875rem;
		margin-bottom: 12px;
	}

	/* Chain filter: one segment per chain, as wide as its share of the results. Each
	   segment is a small tag (logo on top, its count below in the price numeral) and
	   never gets narrower than that content, so a low-count chain can't clip its figure. */
	.share {
		display: flex;
		flex-wrap: wrap;
		gap: 3px;
	}
	.seg {
		--chain: var(--rule);
		box-sizing: border-box;
		flex: 1 1 0;
		min-width: 76px;
		min-height: 62px;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		justify-content: space-between;
		gap: 6px;
		padding: 8px 10px 9px;
		border: 0;
		border-top: 5px solid var(--chain);
		border-radius: 0 0 2px 2px;
		background: var(--label);
		color: var(--ink);
		text-align: left;
		overflow: hidden;
		transition: flex-grow 400ms var(--hang);
	}
	button.seg:hover {
		background: #f4f5f3;
	}
	.seg .num {
		font-size: 1.625rem;
	}
	/* A status in place of the count; it may wrap to a second line rather than be cut. */
	.seg span {
		font-size: 0.75rem;
		font-weight: 700;
		font-stretch: 85%;
		line-height: 1.15;
		color: var(--ink-2);
	}
	.seg[aria-pressed='false'] {
		background: transparent;
		box-shadow: inset 0 0 0 1.5px var(--rule);
		color: var(--ink-2);
		text-decoration: line-through;
	}
	.seg[aria-pressed='false'] :global(img) {
		filter: grayscale(1);
		opacity: 0.6;
	}
	.seg.down span {
		color: var(--offer-ink);
	}
	/* The chains without prices get their own row under the priced ones. */
	.seg.none {
		flex: 1 0 100%;
		flex-direction: row;
		align-items: center;
		justify-content: flex-start;
		gap: 8px;
		min-height: 0;
		height: 36px;
		padding: 0 12px;
		background: repeating-linear-gradient(135deg, #d6d9d5 0 6px, #ccd0cb 6px 12px);
		cursor: help;
	}
	.seg.none span {
		font-size: 0.8125rem;
		font-stretch: 100%;
		color: #3e4244;
	}
	.seg :global(.logo) {
		height: 14px;
		max-width: 100%;
		object-fit: contain;
		object-position: left center;
	}
	.seg :global(.logo.lidl) {
		height: 22px;
	}
	/* City Gross's logo is a small boxed wordmark; it needs more height to be read. */
	.seg :global(.logo.citygross) {
		height: 18px;
	}
	.count {
		margin-top: 10px;
		font-size: 0.8125rem;
		color: var(--ink-2);
	}

	/* A label hangs from its shelf strip. */
	.label {
		position: relative;
		margin-top: 12px;
		display: grid;
		grid-template-columns: 48px minmax(0, 1fr) auto;
		gap: 4px 16px;
		align-items: start;
		background: var(--label);
		padding: 14px 18px 16px;
		border-radius: 0 0 3px 3px;
		box-shadow: 0 3px 6px rgb(0 0 0 / 0.12);
		animation: hang 480ms var(--hang);
		transform-origin: top left;
		transition: box-shadow 200ms var(--hang);
	}
	.label::before {
		content: '';
		position: absolute;
		left: 0;
		right: 0;
		top: -12px;
		height: 12px;
		border-radius: 2px 2px 0 0;
		background: linear-gradient(#61666a, var(--steel-edge));
	}
	.label:hover,
	.label:focus-within {
		box-shadow: 0 8px 18px rgb(0 0 0 / 0.18);
	}
	.thumb {
		width: 48px;
		height: 48px;
		object-fit: contain;
		background: var(--label);
		border-radius: 2px;
	}
	span.thumb {
		background: #f1f2f0;
	}
	@keyframes hang {
		from {
			transform: translateY(-6px) rotate(-0.5deg);
		}
	}
	.name {
		color: inherit;
		font-weight: 700;
		font-size: 1rem;
		line-height: 1.25;
		text-decoration: none;
		text-wrap: pretty;
	}
	.name:hover {
		text-decoration: underline;
		text-underline-offset: 3px;
	}
	.meta {
		margin-top: 2px;
		font-size: 0.8125rem;
		color: var(--ink-2);
	}
	.store {
		margin-top: 8px;
		font-size: 0.8125rem;
		font-weight: 600;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.logo {
		height: 13px;
		width: auto;
		flex-shrink: 0;
	}
	.logo.lidl {
		height: 20px;
	}
	.logo.citygross {
		height: 18px;
	}
	.price {
		text-align: right;
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		white-space: nowrap;
	}
	.num {
		font-weight: 900;
		font-stretch: 62%;
		font-size: 2.875rem;
		/* Digits have no descenders: trim the box to the figures so the unit price sits close. */
		line-height: 0.78;
		letter-spacing: -0.02em;
		font-variant-numeric: tabular-nums lining-nums;
	}
	@supports (text-box: trim-both cap alphabetic) {
		.num {
			line-height: 1;
			text-box: trim-both cap alphabetic;
		}
	}
	.figure {
		display: flex;
		align-items: flex-end;
		gap: 2px;
	}
	.per {
		font-size: 0.875rem;
		font-weight: 800;
		font-stretch: 80%;
	}
	.answer .per {
		font-size: 1.125rem;
	}
	.approx {
		align-self: center;
		font-size: 1.25rem;
		font-weight: 700;
		margin-right: 2px;
	}
	.num sup {
		font-size: 0.46em;
		vertical-align: top;
		position: relative;
		top: 0.1em;
		margin-left: 0.05em;
		letter-spacing: 0;
	}
	.unit {
		margin-top: 8px;
		font-size: 0.875rem;
		font-weight: 700;
		font-stretch: 85%;
		font-variant-numeric: tabular-nums;
	}
	.price s {
		font-size: 0.8125rem;
		color: var(--offer-ink);
		margin-top: 2px;
	}
	.sale {
		font-size: 0.6875rem;
		font-weight: 800;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: #fff;
		background: var(--offer);
		padding: 2px 6px;
		border-radius: 2px;
		margin-bottom: 4px;
	}
	.deal {
		margin-top: 10px;
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 4px 8px;
		font-size: 0.8125rem;
		font-variant-numeric: tabular-nums;
	}
	.deal-tag {
		background: var(--offer);
		color: #fff;
		font-weight: 800;
		font-stretch: 85%;
		padding: 2px 7px;
		border-radius: 2px;
	}
	.deal small {
		flex-basis: 100%;
		font-size: 0.75rem;
		color: var(--ink-2);
	}

	/* The answer: the yellow label. */
	.answer-head {
		margin: 26px 0 2px;
		font-size: 1.375rem;
		font-weight: 800;
		font-stretch: 80%;
	}
	.answer {
		background: var(--yellow);
		padding: 18px 22px 20px 18px;
		grid-template-columns: 64px minmax(0, 1fr) auto;
		box-shadow: 0 6px 14px rgb(0 0 0 / 0.16);
	}
	.answer .thumb {
		width: 64px;
		height: 64px;
		border-radius: 3px;
	}
	.answer .name {
		font-size: 1.1875rem;
	}
	.answer .meta,
	.answer .deal small,
	.answer .when {
		color: var(--yellow-ink);
	}
	.answer .num {
		font-size: clamp(4rem, 7vw, 6rem);
	}
	.answer .unit {
		font-size: 1rem;
	}
	.when {
		font-size: 0.8125rem;
		font-weight: 700;
		margin-top: 4px;
	}
	.wide {
		grid-column: 1 / -1;
		/* Long product names must truncate rather than widen the label's price column. */
		min-width: 0;
	}
	.answer .deal {
		margin-top: 4px;
	}
	.answer {
		margin-top: 18px;
	}
	.best {
		margin-top: 12px;
		padding-top: 12px;
		border-top: 1.5px dashed rgb(38 41 43 / 0.4);
	}
	.best h3 {
		font-size: 0.8125rem;
		font-weight: 700;
		color: var(--yellow-ink);
		margin-bottom: 6px;
	}
	.best ul {
		list-style: none;
		padding: 0;
		margin: 0;
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: 4px;
		font-size: 0.875rem;
	}
	.best li {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.best-logo {
		width: 64px;
		flex-shrink: 0;
		display: flex;
	}
	.best-name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.best b,
	.best small {
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.best small {
		font-weight: 500;
		color: var(--yellow-ink);
	}

	/* All results. */
	.list-head {
		margin: 34px 0 0;
		font-size: 1.375rem;
		font-weight: 800;
		font-stretch: 80%;
	}
	.ranking {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px 20px;
		margin-top: 14px;
		font-size: 0.875rem;
	}
	.basis {
		font-size: 0.8125rem;
		color: var(--ink-2);
		margin-bottom: 2px;
	}
	.check {
		display: flex;
		align-items: center;
		gap: 6px;
		cursor: pointer;
	}
	.check input {
		width: 16px;
		height: 16px;
		margin: 0;
	}
	.sort {
		display: flex;
		border: 0;
		padding: 0;
		margin: 0;
	}
	.sort label {
		position: relative;
		padding: 7px 12px;
		border: 1.5px solid var(--steel);
		font-weight: 600;
		font-size: 0.8125rem;
		cursor: pointer;
		background: var(--label);
	}
	.sort label + label {
		border-left: 0;
	}
	.sort label:first-of-type {
		border-radius: 3px 0 0 3px;
	}
	.sort label:last-of-type {
		border-radius: 0 3px 3px 0;
	}
	.sort label:has(input:checked) {
		background: var(--steel);
		color: #fff;
	}
	.sort label:has(input:focus-visible) {
		outline: 3px solid var(--steel);
		outline-offset: 2px;
	}
	.sort input {
		position: absolute;
		opacity: 0;
		pointer-events: none;
	}
	.shelves {
		list-style: none;
		padding: 0;
		margin: 0;
		display: grid;
		gap: 10px;
	}
	.shelves .label {
		margin-top: 12px;
	}
	.divider {
		margin-top: 18px;
		font-size: 0.8125rem;
		font-weight: 700;
		color: var(--ink-2);
	}
	.others {
		margin-top: 30px;
	}
	.others summary {
		cursor: pointer;
		font-size: 1rem;
		font-weight: 700;
		padding: 10px 0;
	}
	.others summary small {
		display: block;
		font-weight: 400;
		font-size: 0.8125rem;
		color: var(--ink-2);
		margin: 2px 0 0 1.1em;
	}
	.label.empty {
		display: block;
		padding: 24px;
		color: var(--ink-2);
		text-align: center;
	}

	/* Before the first search. */
	.intro {
		padding: 22px 0 8px;
	}
	.intro h2 {
		font-size: clamp(2.5rem, 4.4vw, 3.5rem);
		font-weight: 900;
		font-stretch: 68%;
		line-height: 0.95;
		letter-spacing: -0.01em;
		text-wrap: balance;
	}
	.intro p {
		margin-top: 16px;
		max-width: 52ch;
		font-size: 1.0625rem;
		line-height: 1.5;
	}
	.intro .fine {
		font-size: 0.875rem;
		color: var(--ink-2);
	}
	.try {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 16px 10px;
		margin-top: 28px;
	}
	.try > span {
		font-weight: 700;
		font-size: 0.875rem;
		margin-right: 4px;
	}
	.tag {
		border: 0;
		border-top: 5px solid var(--steel-edge);
		border-radius: 0 0 3px 3px;
		background: var(--label);
		padding: 8px 16px 9px;
		font-weight: 700;
		font-size: 1rem;
		box-shadow: 0 2px 4px rgb(0 0 0 / 0.12);
		transition:
			box-shadow 200ms var(--hang),
			transform 200ms var(--hang);
	}
	.tag:hover {
		background: #f4f5f3;
		box-shadow: 0 4px 10px rgb(0 0 0 / 0.16);
		transform: translateY(1px);
	}

	footer {
		margin: 36px 0 0;
		padding: 18px 0 24px;
		border-top: 1px solid #c4c8c5;
		font-size: 0.75rem;
		line-height: 1.5;
		color: var(--ink-2);
		max-width: 68ch;
	}

	/* Phones and narrow windows: one scrolling page, the map between the answer and the list. */
	@media (max-width: 900px) {
		.rail {
			flex-wrap: wrap;
			padding: 12px 16px 14px;
		}
		.mark {
			font-size: 1.5rem;
		}
		.search {
			order: 3;
			flex-basis: 100%;
			max-width: none;
		}
		.where {
			order: 4;
			flex-basis: 100%;
			margin-left: 0;
			font-size: 0.8125rem;
		}
		.near {
			max-width: 100%;
			flex-basis: 100%;
		}
		.address {
			padding: 12px 16px 16px;
		}
		.address-row {
			flex-wrap: wrap;
		}
		.address-row input {
			flex-basis: 100%;
		}
		.app {
			height: auto;
		}
		.layout {
			display: grid;
			grid-template-columns: minmax(0, 1fr);
			grid-template-areas: 'summary' 'map' 'results' 'footer';
			padding: 16px 16px 0;
		}
		.col {
			display: contents;
		}
		.summary {
			grid-area: summary;
		}
		.results {
			grid-area: results;
		}
		footer {
			grid-area: footer;
		}
		.mapcol {
			grid-area: map;
			height: 46vh;
			min-height: 280px;
			margin: 24px -16px 0;
			border-left: 0;
			border-block: 4px solid var(--steel-edge);
		}
		/* Three segments per row on a phone (six chains make two even rows); one row of six
		   would shrink the logos past reading. The pixel of slack keeps rounding from
		   wrapping the third segment. */
		.seg {
			min-width: calc((100% - 7px) / 3);
		}

		.label {
			grid-template-columns: 40px minmax(0, 1fr) auto;
			gap: 4px 12px;
			padding: 12px 14px 14px 12px;
		}
		.thumb {
			width: 40px;
			height: 40px;
		}
		.num {
			font-size: 2.375rem;
		}
		.answer {
			grid-template-columns: 48px minmax(0, 1fr) auto;
			padding: 16px 16px 18px 12px;
		}
		.answer .thumb {
			width: 48px;
			height: 48px;
		}
		.best small {
			display: none;
		}
		.answer .num {
			font-size: 4rem;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.label {
			animation: none;
		}
		.seg,
		.tag {
			transition: none;
		}
	}
</style>
