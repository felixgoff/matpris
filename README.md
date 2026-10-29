# Matpris

Search for a grocery product and see one merged, price-sorted list from Swedish chains near you, with a map of the stores and a "cheapest near you" pick.

Built with SvelteKit + TypeScript. Apple MapKit JS for the map, OpenStreetMap Nominatim for address lookup.

```bash
bun install
cp .env.example .env   # then paste your MapKit JS token into PUBLIC_MAPKIT_TOKEN
bun run dev        # http://localhost:5173
bun run test       # unit tests (price parsing, search radius)
bun run check      # type check
```

## Map token

The map uses [MapKit JS](https://developer.apple.com/maps/web/) and needs a JWT in `PUBLIC_MAPKIT_TOKEN` (`.env`, which is gitignored). The token is sent to the browser by design, but keep it out of git so it can be rotated.

Tokens made in the Apple Developer dashboard expire. When the map shows "Kartan kunde inte laddas", generate a new one and update `.env`; no rebuild is needed in dev, just a restart. For production, sign short-lived tokens on the server from your MapKit `.p8` key (ES256, with your Team ID as `iss` and the Key ID as `kid`), and set an `origin` claim to your domain.

## How it works

1. **Location.** On load the map turns on MapKit's user location (`showsUserLocation`), and the first location fix moves the search there. Until then, or if location access is denied, it shows central Lund. Entering an address (geocoded through Nominatim) stops following the device; "Använd min position" resumes it.
2. **Automatic search radius** (`src/lib/server/stores/radius.ts`). Starts at 1.5 km and grows ×1.6 until the circle holds at least 12 stores from at least 3 chains, up to a 30 km cap. In a city centre the circle stays small; in the countryside it widens. At most 5 stores per chain and 25 in total are queried.
3. **Live search** (`src/lib/server/search.ts`). All chains are queried in parallel. Results stream to the browser as NDJSON (`/api/search`), so fast chains appear first. Prices are cached for 15 minutes and store lists for 24 hours.
4. **Normalising** (`src/lib/server/normalize.ts`). Prices and comparison prices (kr/kg, kr/l, kr/st) are parsed from each chain's format. Irrelevant hits are dropped: every query word must appear in the product name, and Swedish compounds count, so "mjölk" matches "Mellanmjölk". Plural and definite queries are reduced to their base form, so "bananer" finds "Banan". Each hit then gets a relevance tier: **best** when it is the thing searched for ("Banan Klass 1"), **mention** when the query only flavours or fills another product type or baby food ("Banan & Vanilj Yoghurt"), **weak** when the query only starts a longer word ("Bananchips"). Chains that publish category paths (City Gross, Coop) hand those to the tier logic; the hand-kept list of product-type words fills the rest. Only the top tier is ranked and can be "Cheapest near you"; the rest fold into an "other products" list. Products sold by weight are priced per kg, with a piece price estimated from Willys'/Hemköp's average weight.

### API

| Endpoint | Returns |
| --- | --- |
| `GET /api/search?q=mjölk&lat=59.33&lng=18.06` | NDJSON stream: `area` event, then one `chain` event per chain, then `done` |
| `GET /api/stores?lat=…&lng=…` | The automatically chosen circle and its stores |
| `GET /api/geocode?q=Storgatan 1, Umeå` | `{ lat, lng, label }` |

## Supported chains

| Chain | Products | Store locations | Price scope |
| --- | --- | --- | --- |
| Willys | Public site search JSON (`willys.se/search`) | `willys.se/axfood/rest/store` | National online price |
| Hemköp | Same Axfood platform (`hemkop.se/search`) | `hemkop.se/axfood/rest/store` | Online price of the site's default store |
| Lidl | Public site search (`lidl.se/q/api/search`): current assortment and weekly deals only | `src/lib/server/data/lidl-stores.json`, built from Lidl's public store sitemap | National |
| City Gross | Site search JSON (`citygross.se/api/v1/Loop54/search`), with each product's category | `citygross.se/api/v1/PageData/stores` | Online price of the site's default store |
| Coop | The web shop's own APIM API: `external.api.coop.se/personalization/search/global` (search) and `…/search/entities/by-attribute` (per category) | `proxy.api.coop.se/external/store/stores/map` | National online store (`store=251300`) |
| ICA | *(planned)* a locally assembled catalogue index, `src/lib/server/data/ica-catalogue.json`, built like Lidl's store list | `src/lib/server/data/ica-stores.json`, built from `handla.ica.se/api/store/v1` | —, see below |

Rebuild the Lidl store list now and then (it takes about 4 minutes and makes about 1 request per second):

```bash
bun run stores:lidl
```

Rebuild Coop's and ICA's lists the same way (`bun run stores:ica` needs Playwright and a Chromium, `npx playwright install chromium`):

```bash
bun run stores:ica
```

City Gross's robots.txt disallows the `/loop54/` page route; the `/api/v1/` JSON the shop itself calls is not covered by it. Requests are spaced 2 s apart. Members-only offers ("Klubbpris" in City Gross, "Medlemspris" in Coop) are left out because they need an account.

### Coop prices: the shop's own API

Coop's web shop talks to its APIM API with an `Ocp-Apim-Subscription-Key`, and the keys it hands to every visitor in its page source are used as a browser would (`external.api.coop.se` for search, `proxy.api.coop.se` for the store list); `src/lib/server/chains/coop.ts` fetches them from coop.se like the site's own apps do and caches them for a day. Search uses `personalization/search/global`, the same endpoint the site's search page calls; requests are spaced 2 s apart. Prices are one national list for the online store (store=251300), like the Axfood chains, and deals ("Läsk 2 för 29", single-item offers) are parsed into `deal`/`oldPrice`. MedMera member prices are skipped.

### ICA prices: not reachable (AWS WAF with visual CAPTCHA)

ICA's shop (`handla.ica.se`, `handlaprivatkund.ica.se`) sits behind an AWS WAF. As of 2026-10-29, autommated traffic is answered with either a silent JS challenge (whose cookie doesn't carry over to server-side HTTP) or a **visual image-grid CAPTCHA** ("Bekräfta att du är en människa") that even appears in an in-browser session on the search journey. Solving captchas is out of scope, so there is no automated price source. The groundwork that exists:

- `bun run stores:ica` builds ICA's full store list (356 online stores, with `accountId`, coordinates, address) from the store selector's own JSON, fetched once through a real Chromium page: `src/lib/server/data/ica-stores.json`.
- `scripts/ica-catalogue.mjs` crawls one store's priced assortment by browsing category pages in Chromium and harvesting the JSON the site's own app receives (the approach matval uses for ICA). It is resumable and **aborts at the first CAPTCHA** instead of solving it. Run it from a clean IP with gentle pacing; a heavy probing session (like building this) can get the IP's WAF reputation burned for a while. North of the CAPTCHA it feeds `ica-catalogue.json`, from which an ICA chain adapter (needle-in-index, matching matval's model) can be built the same way Lidl's prebuilt store list works.

### Multi-buy deals

Willys, Hemköp, City Gross and Coop deals such as "5 för 55 kr" are parsed into a structured `deal`: quantity, price each, total, the deal's unit price, mix & match, deposit on top, and the per-customer limit. The list shows the deal under the regular price. With **Count multi-buy deals** on (the default), sorting and the "Cheapest near you" card use the deal price and say how many you need to buy.

### Adding a chain

1. Create `src/lib/server/chains/<chain>.ts` implementing `ChainAdapter` (`listStores`, `search`, `pricing`).
2. Register it in `src/lib/server/chains/index.ts`, and add it to `CHAINS` in `src/lib/types.ts` with `online: true` and a logo in `static/logos/`.

## Logos

The chain logos in `static/logos/` come from the chains' own websites, except ICA's, which is from Wikimedia Commons because ica.se blocks automated requests. They're used only to identify each chain's prices and stores. They are trademarks of their owners; check that this use is OK before launching publicly.

## Being a good citizen (read before deploying)

The data comes from the chains' own websites, not from official APIs, so check each site's terms of use before you run this publicly.

- Every request sends an identifying `User-Agent`.
- Requests are spaced per host (`src/lib/server/http.ts`). Willys and Hemköp get **10 s** between requests, which honours the `Crawl-delay: 10` in their robots.txt. If a chain's queue gets too long, the user sees "busy, try again" for that chain instead of hammering the site. The 15-minute cache means popular searches cost nothing extra.
- Willys' and Hemköp's robots.txt also state `Visit-time: 0400-0845` UTC, a preferred crawl window. Live, user-triggered searches outside that window go against that preference. For a public deployment, consider prefetching common products inside that window, or asking Axfood for access.
- Nominatim's usage policy allows at most 1 request per second, and results are cached for 7 days.

## Deploying

`adapter-auto` works on Vercel, Netlify and similar hosts. The caches live in memory, so a single long-running Node server (`@sveltejs/adapter-node`) gets much more out of them than short-lived serverless functions do.
