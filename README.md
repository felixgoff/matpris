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

1. **Location.** Browser geolocation, or an address geocoded through Nominatim. The default is central Stockholm.
2. **Automatic search radius** (`src/lib/server/stores/radius.ts`). Starts at 1.5 km and grows ×1.6 until the circle holds at least 12 stores from at least 3 chains, up to a 30 km cap. In a city centre the circle stays small; in the countryside it widens. At most 5 stores per chain and 25 in total are queried.
3. **Live search** (`src/lib/server/search.ts`). All chains are queried in parallel. Results stream to the browser as NDJSON (`/api/search`), so fast chains appear first. Prices are cached for 15 minutes and store lists for 24 hours.
4. **Normalising** (`src/lib/server/normalize.ts`). Prices and comparison prices (kr/kg, kr/l, kr/st) are parsed from each chain's format. Irrelevant hits are dropped: every query word must appear in the product name, and Swedish compounds count, so "mjölk" matches "Mellanmjölk".

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

Rebuild the Lidl store list now and then (it takes about 4 minutes and makes about 1 request per second):

```bash
bun run stores:lidl
```

### Not included: ICA and Coop

- **ICA** prices are per store, and its online shop (`handlaprivatkund.ica.se`) blocks automated requests (HTTP 403). The site doesn't try to get around that.
- **Coop**'s product API requires a subscription key. The site doesn't pull that key out of Coop's website.

To add either chain properly, ask for API or data access, or use a licensed price-data provider. Then implement an adapter (see below) with `pricing: 'store'`. The search already queries every store of a per-store chain inside the circle and keeps each product's cheapest offer.

### Adding a chain

1. Create `src/lib/server/chains/<chain>.ts` implementing `ChainAdapter` (`listStores`, `search`, `pricing`).
2. Register it in `src/lib/server/chains/index.ts`, and add its name and colour to `CHAINS` in `src/lib/types.ts`.

## Being a good citizen (read before deploying)

The data comes from the chains' own websites, not from official APIs, so check each site's terms of use before you run this publicly.

- Every request sends an identifying `User-Agent`.
- Requests are spaced per host (`src/lib/server/http.ts`). Willys and Hemköp get **10 s** between requests, which honours the `Crawl-delay: 10` in their robots.txt. If a chain's queue gets too long, the user sees "busy, try again" for that chain instead of hammering the site. The 15-minute cache means popular searches cost nothing extra.
- Willys' and Hemköp's robots.txt also state `Visit-time: 0400-0845` UTC, a preferred crawl window. Live, user-triggered searches outside that window go against that preference. For a public deployment, consider prefetching common products inside that window, or asking Axfood for access.
- Nominatim's usage policy allows at most 1 request per second, and results are cached for 7 days.

## Deploying

`adapter-auto` works on Vercel, Netlify and similar hosts. The caches live in memory, so a single long-running Node server (`@sveltejs/adapter-node`) gets much more out of them than short-lived serverless functions do.
