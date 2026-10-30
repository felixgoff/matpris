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
| ICA | The shop's own search API `handlaprivatkund.ica.se/stores/{id}/api/webproductpagews/v6/product-pages/search?q=`, with a WAF session cookie refreshed by `scripts/ica-warm.mjs` | `src/lib/server/data/ica-stores.json`, built from `handla.ica.se/api/store/v1` | Prices of Maxi ICA Gunnesbo, Lund (store 1088004), shown as chain pricing |

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

### ICA prices: the shop's own search API with a maintained WAF session

ICA's shop (`handlaprivatkund.ica.se`) sits behind an AWS WAF. The decisive point (worked out 2026-10-29): the WAF answer depends on the **client IP's reputation**, not on automation as such. After a period of gentle browsing from a clean residential IP, the shop's own search API answers full product JSON to a plain server-side `fetch` — if the request carries the `aws-waf-token` session cookie the token-minting JS challenge has issued. The token is short-lived, which is the only maintenance this needs:

- `bun run warm:ica` (`scripts/ica-warm.mjs`) opens the shop in the persistent Chrome profile (`scripts/.ica-profile/`) — headless is fine when the IP is clean — and saves the current cookies to `.ica-profile/session.json`. It exits within seconds once the token is minted.
- `src/lib/server/chains/ica.ts` loads those cookies for every search, and when the session gets stale (tokens last ~4 minutes) or the WAF answers a challenge (HTTP 202), it re-runs the warm script automatically, once, and retries. If the WAF still issues its **visual image-grid CAPTCHA** after that (which follows heavy probing and a burned IP reputation; it cools off after minutes to hours), the search returns "throttled" instead of pretending to have prices. Solving CAPTCHAs programmatically stays out of scope by design: a visual puzzle is cleared by a human running `HEADED=1 bun run warm:ica` and clicking it — the same rule the catalogue crawler follows.
- Search requests ride on the same cookie jar with a matching browser User-Agent (@matval's ICA spider pattern: mint the token with a small Playwright run, read the JSON APIs server-side in between; its 2025 logs show 33k requests with zero challenges).
- Prices anchor to one store (Maxi ICA Stormarknad Gunnesbo, Lund) and are shown for the whole chain, like Lidl's list. Deals (`promoPrice` below the regular price) surface as the current price with the regular one as the old price.
- `bun run record:ica` opens the shop for a human to browse by hand and logs every API call the site makes (URLs, parameters, response snippets) to `.ica-profile/journey.jsonl` — the tool that produced the endpoint recon above.

Backup plan: `scripts/ica-catalogue.mjs` pre-builds a full priced assortment index for one store (`data/ica-catalogue.json`, resumable, pauses for a human on CAPTCHA), which an Lidl-style index adapter can search if live requests ever stop working.

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

## Desktop app (macOS)

The same app as a small native Mac app, built with [Electrobun](https://framework.blackboard.sh/electrobun/) 2:

```bash
bun run desktop        # build, then open a dev window (console output in the terminal)
bun run desktop:build  # release app: desktop/build/stable-macos-arm64/Matpris.app (~32 MB)
```

`vite build` produces the SvelteKit server (`adapter-node`, in `build/`). Electrobun copies it into the app, and `desktop/main.ts`, running in the app's Bun process, starts it on a free `127.0.0.1` port and opens a window on it. Everything else works as on the web: searches, caches and request spacing all live in that server. The window uses macOS's own WebKit, not a bundled Chrome. Device location asks for permission the first time.

- **Map token:** the app has no `.env`, so `electrobun.config.ts` builds `PUBLIC_MAPKIT_TOKEN` in from `.env` at build time. Rebuild after renewing the token. It's a public token, but don't hand the app around.
- **ICA** shows "busy" in the app: its session files and warm script live in `scripts/` on this Mac, outside the app bundle.
- **Icon:** `desktop/icons/Matpris.icon`, an Icon Composer document (open it in Icon Composer to adjust the glass, shadows or colours). The other candidates and a comparison sheet are in `desktop/icons/drafts/`.
- The first build downloads Electrobun's runtime into `~/.hutch`. The app isn't code-signed, so macOS may ask before opening it the first time.

## Deploying

`vite build` produces a standalone Node server (`@sveltejs/adapter-node`, `node build`). The caches live in memory, so one long-running server gets much more out of them than short-lived serverless functions would.
