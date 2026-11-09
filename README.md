# Matpris

Search for a grocery product and see one merged, price-sorted list from Swedish chains near you, with a map of the stores and a "cheapest near you" pick.

One app for macOS, Windows, Linux, Android and iPhone, built with [Tauri 2](https://tauri.app): the page is SvelteKit + TypeScript, and everything behind it (searching the chains, ranking, finding stores) is a Rust backend that runs inside the app on each device. A Galaxy Watch app asks the phone. Apple MapKit JS draws the map, OpenStreetMap Nominatim looks up addresses.

```bash
bun install
cp .env.example .env   # then paste your MapKit JS token into PUBLIC_MAPKIT_TOKEN
bun run dev            # the page in a browser at http://localhost:5173, with the Rust dev server
bun run app            # the desktop app (Tauri dev window)
bun run test           # Rust tests: parsing, relevance, search circle, every chain's mapping
bun run check          # type check the page
```

| Folder | What it is |
| --- | --- |
| `core/` | The backend (crate `matpris-core`): chain adapters, request spacing, cache, relevance, store circle, search stream, geocoding |
| `src/` | The page (SvelteKit, built static). `src/lib/api.ts` reaches the backend: Tauri commands in the app, `/api/*` in a browser |
| `src-tauri/` | The Tauri app: commands, ICA's session window, desktop self-update, the Android project (`gen/android`) and the watch bridge |
| `plugins/ica-session/` | Tauri mobile plugin (Kotlin, Swift) that gets ICA's session cookie on phones |
| `server/` | The backend over HTTP (`/api/search` etc.) for developing the page in a browser (`bun run dev` starts it) |
| `wear/` | The Galaxy Watch / Wear OS app (Kotlin, Compose for Wear OS) |
| `scripts/` | Store-list builders and ICA's session tools (Node + Playwright) |

## Map token

The map uses [MapKit JS](https://developer.apple.com/maps/web/) and needs a JWT in `PUBLIC_MAPKIT_TOKEN` (`.env`, which is gitignored). The token is sent to the browser by design, but keep it out of git so it can be rotated.

Tokens made in the Apple Developer dashboard expire. When the map shows "The map could not be loaded", generate a new one, update `.env` (and the `PUBLIC_MAPKIT_TOKEN` repo secret), and rebuild: the token is built into the page, because the apps have no server to read it from. For production, sign short-lived tokens on the server from your MapKit `.p8` key (ES256, with your Team ID as `iss` and the Key ID as `kid`), and set an `origin` claim to your domain.

## How it works

1. **Location.** On load the map turns on MapKit's user location (`showsUserLocation`), and the first location fix moves the search there. Until then, or if location access is denied, it shows central Lund. Entering an address (geocoded through Nominatim) stops following the device; "Use my location" resumes it. In the apps the system asks for location permission the first time (Android: the webview's prompt becomes the system one; iOS and macOS: the usage text in `Info.ios.plist` / `Info.plist`).
2. **Automatic search radius** (`core/src/stores/radius.rs`). Starts at 1.5 km and grows ×1.6 until the circle holds at least 12 stores from at least 3 chains, up to a 30 km cap. In a city centre the circle stays small; in the countryside it widens. At most 5 stores per chain and 25 in total are queried.
3. **Live search** (`core/src/search.rs`). All chains are queried in parallel. Results stream to the page as they arrive (a Tauri channel in the app, NDJSON from `/api/search` in a browser), so fast chains appear first. Prices are cached for 15 minutes and store lists for 24 hours.
4. **Normalising** (`core/src/normalize.rs`). Prices and comparison prices (kr/kg, kr/l, kr/st) are parsed from each chain's format. Irrelevant hits are dropped: every query word must appear in the product name, and Swedish compounds count, so "mjölk" matches "Mellanmjölk". Plural and definite queries are reduced to their base form, so "bananer" finds "Banan". Each hit then gets a relevance tier: **best** when it is the thing searched for ("Banan Klass 1"), **mention** when the query only flavours or fills another product type or baby food ("Banan & Vanilj Yoghurt"), **weak** when the query only starts a longer word ("Bananchips"). Chains that publish category paths (City Gross, Coop) hand those to the tier logic; the hand-kept list of product-type words fills the rest. Only the top tier is ranked and can be "Cheapest near you"; the rest fold into an "other products" list. Products sold by weight are priced per kg, with a piece price estimated from Willys'/Hemköp's average weight.

### API

The app's commands (`src-tauri/src/lib.rs`: `search`, `stores`, `geocode`, `image`) and the dev server's routes (`server/`) answer the same things:

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
| Lidl | Public site search (`lidl.se/q/api/search`): current assortment and weekly deals only | `core/data/lidl-stores.json`, built from Lidl's public store sitemap | National |
| City Gross | Site search JSON (`citygross.se/api/v1/Loop54/search`), with each product's category | `citygross.se/api/v1/PageData/stores` | Online price of the site's default store |
| Coop | The web shop's own APIM API: `external.api.coop.se/personalization/search/global` (search) | `proxy.api.coop.se/external/store/stores/map` | National online store (`store=251300`) |
| ICA | The shop's own search API `handlaprivatkund.ica.se/stores/{id}/api/webproductpagews/v6/product-pages/search?q=`, with a WAF session cookie refreshed by `scripts/ica-warm.mjs` | `core/data/ica-stores.json`, built from `handla.ica.se/api/store/v1` | Prices of Maxi ICA Gunnesbo, Lund (store 1088004), shown as chain pricing |

Rebuild the Lidl store list now and then (it takes about 4 minutes and makes about 1 request per second):

```bash
bun run stores:lidl
```

Rebuild ICA's and the Axfood chains' lists the same way (`bun run stores:ica` needs Playwright and a Chromium, `npx playwright install chromium`). Coop's and City Gross's stores are fetched live and cached for a day.

```bash
bun run stores:ica
bun run stores:axfood   # Willys and Hemköp, one request per site
```

City Gross's robots.txt disallows the `/loop54/` page route; the `/api/v1/` JSON the shop itself calls is not covered by it. Requests are spaced 2 s apart. Members-only offers ("Klubbpris" in City Gross, "Medlemspris" in Coop) are left out because they need an account.

### Coop prices: the shop's own API

Coop's web shop talks to its APIM API with an `Ocp-Apim-Subscription-Key`, and the keys it hands to every visitor in its page source are used as a browser would (`external.api.coop.se` for search, `proxy.api.coop.se` for the store list); `core/src/chains/coop.rs` fetches them from coop.se like the site's own apps do and caches them for a day. Search uses `personalization/search/global`, the same endpoint the site's search page calls; requests are spaced 2 s apart. Prices are one national list for the online store (store=251300), like the Axfood chains, and deals ("Läsk 2 för 29", single-item offers) are parsed into `deal`/`oldPrice`. MedMera member prices are skipped.

### ICA prices: the shop's own search API with a maintained WAF session

ICA's shop (`handlaprivatkund.ica.se`) sits behind an AWS WAF. The decisive point (worked out 2026-10-29): the WAF answer depends on the **client IP's reputation**, not on automation as such. After a period of gentle browsing from a clean residential IP, the shop's own search API answers full product JSON to a plain server-side `fetch` — if the request carries the `aws-waf-token` session cookie the token-minting JS challenge has issued. The token is short-lived, which is the only maintenance this needs:

- `core/src/chains/ica.rs` sends those cookies with every search, and when the session gets stale (tokens last ~4 minutes) or the WAF answers a challenge (HTTP 202), it mints a fresh one, once, and retries. Who mints it depends on where the backend runs (a `CookieJar`):
  - **Desktop app** (`src-tauri/src/ica.rs`): opens the shop in a hidden window and reads its cookies once a *fresh* `aws-waf-token` appears (WebKit keeps an old one on disk that the WAF has already turned down).
  - **Phones** (`plugins/ica-session`): the same with an offscreen WebView (Android: `CookieManager`; iOS: `WKHTTPCookieStore`).
  - **Dev server** (`server/src/ica_jar.rs`): runs `bun run warm:ica` (`scripts/ica-warm.mjs`), which opens the shop in the persistent Chrome profile `scripts/.ica-profile/` and saves the cookies to `.ica-profile/session.json`.
- If the WAF shows its **visual image-grid CAPTCHA** (after heavy probing or with a burned IP reputation; it cools off after minutes to hours), the app shows that page to the person using it ("ICA wants to check that you're a person"); the dev server opens a visible Chrome (`HEADED=1`). Nothing solves the puzzle programmatically, by design. Until someone does, ICA shows "busy, try again" instead of pretending to have prices.
- Search requests ride on the same cookie jar with a matching browser User-Agent (@matval's ICA spider pattern: mint the token with a small Playwright run, read the JSON APIs server-side in between; its 2025 logs show 33k requests with zero challenges).
- Prices anchor to one store (Maxi ICA Stormarknad Gunnesbo, Lund) and are shown for the whole chain, like Lidl's list. Deals (`promoPrice` below the regular price) surface as the current price with the regular one as the old price.
- `bun run record:ica` opens the shop for a human to browse by hand and logs every API call the site makes (URLs, parameters, response snippets) to `.ica-profile/journey.jsonl` — the tool that produced the endpoint recon above.

Backup plan: `scripts/ica-catalogue.mjs` pre-builds a full priced assortment index for one store (`core/data/ica-catalogue.json`, resumable, pauses for a human on CAPTCHA), which an Lidl-style index adapter can search if live requests ever stop working.

### Multi-buy deals

Willys, Hemköp, City Gross and Coop deals such as "5 för 55 kr" are parsed into a structured `deal`: quantity, price each, total, the deal's unit price, mix & match, deposit on top, and the per-customer limit. The list shows the deal under the regular price. With **Count multi-buy deals** on (the default), sorting and the "Cheapest near you" card use the deal price and say how many you need to buy.

### Adding a chain

1. Create `core/src/chains/<chain>.rs` implementing `ChainAdapter` (`list_stores`, `search`, `pricing`), with its mapping tests.
2. Register it in `adapters()` in `core/src/chains/mod.rs` and `ChainId` in `core/src/types.rs`, and add it to `CHAINS` in `src/lib/types.ts` with `online: true` and a logo in `static/logos/`.

## Logos

The chain logos in `static/logos/` come from the chains' own websites, except ICA's, which is from Wikimedia Commons because ica.se blocks automated requests. They're used only to identify each chain's prices and stores. They are trademarks of their owners; check that this use is OK before launching publicly.

## Being a good citizen (read before deploying)

The data comes from the chains' own websites, not from official APIs, so check each site's terms of use before you run this publicly.

- Every request sends an identifying `User-Agent`.
- Requests are spaced per host (`core/src/http.rs`), on every device: each app spaces its own requests. Willys and Hemköp get **10 s** between requests, which honours the `Crawl-delay: 10` in their robots.txt. If a chain's queue gets too long, the user sees "busy, try again" for that chain instead of hammering the site. The 15-minute cache means popular searches cost nothing extra.
- Willys' and Hemköp's robots.txt also state `Visit-time: 0400-0845` UTC, a preferred crawl window. Live, user-triggered searches outside that window go against that preference. For a public deployment, consider prefetching common products inside that window, or asking Axfood for access.
- Nominatim's usage policy allows at most 1 request per second, and results are cached for 7 days.

## Apps

| Platform | Build | Notes |
| --- | --- | --- |
| macOS, Windows, Linux | `bun run app:build` (on that OS) | Self-updates from GitHub releases |
| Android | `bun run android:build` | `src-tauri/gen/android` is committed |
| iPhone | `bun run ios` on a Mac with Xcode | Needs your Apple developer team |
| Galaxy Watch 5 / Wear OS 3+ | `cd wear && ./gradlew assembleDebug` | Asks the Android phone app ([wear/README.md](wear/README.md)) |

The flat app icon for every platform except the Mac's Icon Composer document is `icons/matpris.svg` (`matpris-foreground.svg` for Android's adaptive icon, `matpris-round.svg` for round screens); `src-tauri/icons/` holds the rendered sizes.

### Desktop (macOS, Windows, Linux)

```bash
bun run app         # dev window, page with hot reload
bun run app:build   # release bundle for this OS, in target/release/bundle/
```

The window is the system's own webview: WebKit on macOS, WebView2 on Windows, WebKitGTK 4.1 on Linux (Ubuntu/Debian: `libwebkit2gtk-4.1-0 libgtk-3-0 libayatana-appindicator3-1 librsvg2-2`; building needs the `-dev` packages and `patchelf`). Tauri builds only for the OS it runs on, so the Release workflow has a runner per OS.

### Phones

```bash
export ANDROID_HOME=… NDK_HOME=$ANDROID_HOME/ndk/<version>
bun run android:build -- --apk --target aarch64   # signed with keystore.properties in src-tauri/gen/android, or debug
bun run android                                     # dev on a connected phone
bun run ios                                         # macOS + Xcode: bunx tauri ios init once, then this
```

The phone runs the same Rust backend as the desktop: no server. Location goes through the system prompt (`ACCESS_FINE_LOCATION` in the Android manifest, `NSLocationWhenInUseUsageDescription` in `src-tauri/Info.ios.plist`). The page leaves room for the status bar, notch and home indicator (`env(safe-area-inset-*)`, and padding in Android's `MainActivity`).

### Galaxy Watch

The watch app sends its search to the phone over the Wear OS Data Layer; `WearSearchService` in the Android app runs it in the Rust backend (`src-tauri/src/watch.rs`, over JNI, even when the app isn't open) and streams the events back. Phone and watch must have the same application id (`se.matpris.matpris`) and be signed with the same key. Details in [wear/README.md](wear/README.md).

### Releases and updates

The version lives in `package.json` only. To ship one:

```bash
bun run bump patch     # or minor, major, or an exact 0.2.0
git commit -am "Version 0.0.2" && git push
```

or run **Actions › Bump version**. The **Release** workflow (`desktop-release.yml`) then builds on macOS, Windows and Linux runners plus one Android job (phone + watch), checks that the iPhone app compiles (without signing), and publishes the GitHub release `v<version>` with the installers (`.dmg`, `-setup.exe`/`.msi`, `.AppImage`/`.deb`, the two APKs), the signed update bundles and `latest.json`. Pushes whose version is already released skip the builds.

Set these up once in the repo's settings (each missing one is a warning, not a failed release):

| Name | Kind | For |
| --- | --- | --- |
| `PUBLIC_MAPKIT_TOKEN` | secret | The map |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | secrets | Signing desktop updates: `bunx tauri signer generate -w ~/.tauri/matpris.key` |
| `TAURI_UPDATER_PUBKEY` | variable | The public half of that key, built into the app; without it the app never updates itself |
| `ANDROID_KEYSTORE` (base64), `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | secrets | One key for the phone and watch APKs, so they update in place and can talk to each other |

Installed desktop copies update like the Electrobun app did (`src-tauri/src/update.rs`): on launch a newer release downloads and installs behind a small progress window, then the app restarts as the new version; while it's open it looks every hour, downloads quietly and offers **Restart**. Offline, a slow check (8 s) or any failure just opens the installed version. The repo is private, so the app reads `latest.json` and the update through the GitHub API with the computer's `gh` login (`gh auth login`); the token is kept in memory only, and a computer without gh never updates. Copies of the old Electrobun app don't update to the Tauri one: install it once by hand.
