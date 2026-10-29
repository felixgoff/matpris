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

1. **Location.** On load the map turns on MapKit's user location (`showsUserLocation`), and the first location fix moves the search there. Until then, or if location access is denied, it shows central Stockholm. Entering an address (geocoded through Nominatim) stops following the device; "Använd min position" resumes it.
2. **Automatic search radius** (`src/lib/server/stores/radius.ts`). Starts at 1.5 km and grows ×1.6 until the circle holds at least 12 stores from at least 3 chains, up to a 30 km cap. In a city centre the circle stays small; in the countryside it widens. At most 5 stores per chain and 25 in total are queried.
3. **Live search** (`src/lib/server/search.ts`). All chains are queried in parallel. Results stream to the browser as NDJSON (`/api/search`), so fast chains appear first. Prices are cached for 15 minutes and store lists for 24 hours.
4. **Normalising** (`src/lib/server/normalize.ts`). Prices and comparison prices (kr/kg, kr/l, kr/st) are parsed from each chain's format. Irrelevant hits are dropped: every query word must appear in the product name, and Swedish compounds count, so "mjölk" matches "Mellanmjölk". Plural and definite queries are reduced to their base form, so "bananer" finds "Banan". Each hit then gets a relevance tier: **best** when it is the thing searched for ("Banan Klass 1"), **mention** when the query only flavours or fills another product type or baby food ("Banan & Vanilj Yoghurt", from a hand-kept list of product-type words), **weak** when the query only starts a longer word ("Bananchips"). Only the top tier is ranked and can be "Cheapest near you"; the rest fold into an "other products" list. Products sold by weight are priced per kg, with a piece price estimated from Willys'/Hemköp's average weight.

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

Rebuild the Lidl store list now and then (it takes about 4 minutes and makes about 1 request per second):

```bash
bun run stores:lidl
```

City Gross's robots.txt disallows the `/loop54/` page route; the `/api/v1/` JSON the shop itself calls is not covered by it. Requests are spaced 2 s apart. Members-only offers ("Klubbpris") are left out because they need a City Gross account.

### ICA and Coop: stores only, no prices

ICA and Coop stores appear on the map and in the chain list, marked **"no online prices"**. Their locations come from OpenStreetMap via Nominatim (`src/lib/server/stores/osm.ts`): one lookup per chain per new search circle, cached for 24 hours, spaced at least 1.1 s apart as Nominatim's usage policy requires. For a busy public site, switch to a self-hosted Nominatim or an Overpass extract.

Why there are no prices:

- **ICA** prices are per store. Product pages on `handla.ica.se` show no price until you choose a store, and the store-specific shop (`handlaprivatkund.ica.se`) and `www.ica.se` block automated requests (HTTP 403). The site doesn't try to get around that.
- **Coop**'s store list and prices come from an API that needs a subscription key, and Coop's robots.txt disallows its search pages. Coop runs an official API catalog at https://portal.api.coop.se. If you get a key there, add `COOP_API_KEY` to `.env`, and a Coop adapter (`pricing: 'store'`) can be written against the documented endpoints. The search already queries every store of a per-store chain inside the circle and keeps each product's cheapest offer.
- Open-source scrapers that do fetch ICA and Coop prices (for example matval) solve ICA's AWS WAF challenge with a headless browser and lift Coop's API key out of coop.se. That is getting around the chains' blocking, so this project doesn't do it.

### Multi-buy deals

Willys, Hemköp and City Gross deals such as "5 för 55 kr" are parsed into a structured `deal`: quantity, price each, total, the deal's unit price, mix & match, deposit on top, and the per-customer limit. The list shows the deal under the regular price. With **Count multi-buy deals** on (the default), sorting and the "Cheapest near you" card use the deal price and say how many you need to buy.

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
