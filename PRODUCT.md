# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users
Budget-conscious grocery shoppers in Sweden planning a shop, typically at home on a phone or laptop before the weekly trip. Their job: find out which nearby store has the staples they need at the lowest price, deals included, and decide where to go.

## Product Purpose
Search for a grocery product and get one merged, price-sorted list across Swedish chains near you, with a map of the stores and a "Cheapest near you" pick. Success means a shopper can answer "where should I buy this?" in one search, with prices they can trust.

## Positioning
Live and local: prices are fetched live from the chains' own sites (cached briefly), and results are scoped to stores actually around the user via an automatic search radius rather than national averages. Multi-buy deals are parsed and counted, so "cheapest" reflects what the shopper would really pay.

## Operating Context
- Starts from device location (MapKit user location) or a typed address/town (Nominatim geocoding); falls back to central Stockholm.
- Results stream in per chain, so fast chains appear first and slow or throttled chains show "busy, try again".
- Product names and queries are Swedish (e.g. mjölk, kaffe); UI copy is English.

## Capabilities and Constraints
- Priced chains: Willys, Hemköp (Axfood), Lidl. ICA and Coop stores appear on the map marked "no online prices".
- Desired: attempt to get ICA and/or Coop prices. Current known limits: ICA's store shop and ica.se return 403 to automated requests and prices are per store; Coop's prices need an API subscription key (official catalog at portal.api.coop.se) and its robots.txt disallows search pages. Open decision: which legitimate route to pursue (e.g. Coop API key); the product does not circumvent blocks.
- Good-citizen rules are part of the product: identifying User-Agent, per-host request spacing (10 s for Axfood per Crawl-delay), Nominatim max 1 req/s, caches (prices 15 min, stores 24 h).
- Price scope differs per chain (national online price vs. a default store's online price); the UI must not overstate precision.
- Stack: SvelteKit + TypeScript, Svelte 5, Bun, Apple MapKit JS (token in `.env`).

## Brand Commitments
- Name: Matpris.
- UI language: English, with Swedish product data.

## Evidence on Hand
- Chain logos in `static/logos/` (willys, hemkop, lidl, ica, coop). These are owners' trademarks; use is only to identify each chain, and permission is unchecked before public launch.
- No users, testimonials, press, or usage stats exist. Do not fabricate any.

## Product Principles
1. Honest about coverage: say plainly which chains have prices and which don't, and why.
2. The real price wins: compare what the shopper would actually pay, deals and quantities included.
3. Near means near: results reflect stores the user can realistically reach.
4. Fast first answer: show results as they arrive rather than waiting for the slowest chain.
5. Respect the sources: never trade politeness toward chains' sites for speed.

## Launch Status
Private for now; not publicly launched. Terms of use for each data source and logo use must be checked before any public launch.
