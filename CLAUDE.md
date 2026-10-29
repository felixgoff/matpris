# Matpris – project memory

Swedish grocery price comparison. Search a product, see one merged list of prices from nearby chains, a map of the stores (Apple MapKit JS), and "Cheapest near you". README.md has the full technical description; this file records decisions, constraints and preferences from earlier sessions so they aren't re-litigated.

## Owner preferences
- UI language: **English** (product and store names stay as the chains publish them, i.e. Swedish). Numbers via `src/lib/format.ts` (`19.40 kr`, `1.1 km`).
- Stack: **SvelteKit 3 + TypeScript**, package manager **Bun** (`bun install`, `bun run dev|test|check|build`). Svelte 5 runes.
- Map: **Apple MapKit JS** (not Leaflet). Must fit all stores on screen; map is big (full-height right column on desktop, ~65vh on mobile).
- The owner tests from **Lund** (55.7047, 13.1910); use it for location-based checks.
- Work is pushed directly to `main` on `felixgoff/matpris` (private).
- Claude Code plugins **Impeccable** (design: `/impeccable critique|polish|audit…`) and **Ponytail** (`/ponytail`, `/ponytail-review`) are enabled in `.claude/settings.json`. They load only at session start.

## Running and secrets
- `PUBLIC_MAPKIT_TOKEN` (MapKit JS JWT) is read from the environment at startup (declared in `src/env.ts`, used in `src/lib/mapkit.ts`). Locally it lives in `.env` (gitignored). In cloud sessions it must be set as an environment variable in the environment settings. **Never commit the token.** The current token expires **2026-11-06** and has no origin restriction (MapKit warns in the console). Follow-up idea: sign short-lived tokens server-side from the `.p8` key (Team ID `JWR5UX55S5`, Key ID `W6JH93XCAZ`).
- Without a token the app still works; the map shows "The map could not be loaded".

## Data sources and hard constraints (do not work around)
- **Willys, Hemköp** (Axfood): public JSON `https://www.<site>/search?q=…&size=40`, stores at `/axfood/rest/store?online=false`. One national online price (no unauthenticated store selection), so `pricing: 'chain'`. robots.txt: `Crawl-delay: 10` (enforced per host in `src/lib/server/http.ts`) and `Visit-time: 0400-0845` UTC (not enforced; documented in README).
- **Lidl**: `https://www.lidl.se/q/api/search?...` (returns 401 if `accept: application/json` is sent, so we send `*/*`). Only food categories `Food`/`F+V`. Store list is prebuilt in `src/lib/server/data/lidl-stores.json` from Lidl's store sitemap (`bun run stores:lidl`, ~4 min, 1 req/s).
- **ICA**: no usable public price source. handlaprivatkund.ica.se and www.ica.se return 403 to automated requests; handla.ica.se product pages have no price until a store is chosen. **Do not try to bypass blocking** (browser user agents, extracting keys, etc.).
- **Coop**: prices and store list need an API key. Pulling the key out of coop.se was refused as circumvention; don't do it. The legitimate route is Coop's API catalog at https://portal.api.coop.se: if the owner gets a key, add `COOP_API_KEY` and write an adapter against the documented endpoints (not guessed ones). robots.txt disallows `/handla/search*`.
- ICA and Coop are therefore **map-only** chains (`online: false` in `CHAINS`): their store locations come from OpenStreetMap via Nominatim (`src/lib/server/stores/osm.ts`, cached 24 h per area, ≥1.1 s between requests, identifying User-Agent). Overpass is unreachable from the cloud sandbox.
- Logos in `static/logos/` are from the chains' own sites, except ICA's (Wikimedia Commons, because ica.se blocks us). Coop's is its pinned-tab SVG coloured `#00aa46`, with the viewBox cropped.

## Key behaviours (and why)
- **Auto search radius** (`stores/radius.ts`): start 1.5 km, ×1.6 until ≥12 stores from ≥3 priced chains, cap 30 km; if empty, stretch to the nearest store. Max 5 stores per chain, 25 total. Radius is sized by priced chains only; map-only stores are added inside the same circle afterwards.
- **Search stream** (`server/search.ts`, `/api/search`): NDJSON. The `area` event with priced stores first, then a `chain` event per chain, then a second, complete `area` event with ICA/Coop plus `no-online-prices` chain events. Prices cached 15 min.
- **Relevance** (`normalize.ts` `matchQuality`): whole word or compound head (+ Swedish endings: "bananer", "mjölken") = strong; a word that merely starts with the query = weak. Weak hits are hidden when any strong hit exists. This fixed "fanta" matching "Fantasty" bouillon.
- **Multi-buy deals** (`axfood.ts` `toHit`): Axfood `potentialPromotions[0]` with `qualifyingCount > 1` becomes `deal {quantity, each, total, unitPrice, mixAndMatch, plusDeposit, limit}`; single-item promotions become `price` + `oldPrice` ("Sale"). The UI toggle "Count multi-buy deals" (default on) makes sorting and "Cheapest near you" use the deal price and say "each when you buy N".
- **Location**: MapKit `showsUserLocation` (needs the `user-location` library) moves the search to the device on first fix. Entering an address stops following the device; "Use my location" resumes. Default is Stockholm.
- **Map pins**: custom logo pins with `DisplayPriority.Required` (otherwise MapKit hides colliding pins). The view fits the search circle with pin-sized padding (fitting the pins themselves zooms out too far). `animate: false`, because overlapping animated fits ended at the wrong zoom. ICA/Coop pins are faded.

## Verifying changes
- `bun run check`, `bun run test` (vitest: normalize, radius, Axfood mapping), `bun run build`.
- Browser check: run `bun run dev`, then a Playwright script (Chromium is preinstalled; `require('playwright')`) with a context that grants geolocation set to Lund. Search "fanta" or "kaffe" and check the chips, the Cheapest card, the deal text, that all `.store-pin` elements are inside `.map`'s bounds, that there is no horizontal scroll at 390px, and that there are no page errors. Look at screenshots, not just DOM counts.
- Don't hammer the chains: Axfood allows 1 uncached request per 10 s per host.

## Open ideas
- Coop prices once a portal key exists; server-signed MapKit tokens; Hemköp per-store prices (needs a store-selection method that isn't blocked); prefetch popular searches inside Axfood's visit window.
