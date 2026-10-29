---
version: 1
slug: "src-routes-page-svelte"
primary_target: "src/routes/+page.svelte"
related_targets: ["src/lib/components/StoreMap.svelte"]
---

# Surface: search and results (/)

Mode: Operate. Audience: budget shoppers planning a weekly shop at home on phone or laptop. Task: search a staple, see where it is cheapest nearby, trust the comparison. Success: the shopper trusts the ranking (unit price basis, deals counted, missing chains stated). Pain to fix: generic look, hard to scan, cramped controls, weak phone layout. Flow may be restructured; every feature stays (search, device location, address lookup, chain filters, sort, multi-buy toggle, cheapest pick, per-chain best, map, disclaimer).

## Direction contract

THESIS: Every result is a shelf-edge price label hanging on a steel shelf rail, ranked by the unit price shoppers read in store. Refuses the green card list and a pill search bar. Small product images stay on each label (user's call, 2026-10-29). The large figure is always the one the list is ranked by.

OWN-WORLD: Charcoal steel rail (#26292b) for the header and the thin shelf strips; cool fluorescent grey ground (#e6e8e4); white label stock; label yellow (#ffd400) only on the cheapest answer; offer red (#d4001a) only for deals, sale and errors. Archivo with its width axis: condensed black numerals with raised öre, normal-width body. Square 3px corners, short soft drop shadow under labels.

STORY: The shopper types a staple, sees the yellow label first (price, unit price, store, distance), sees the best price at each chain under it, and trusts it because the ranking basis, deal counting and the chains without prices are stated plainly.

FIRST VIEWPORT: 72px charcoal rail: wordmark, wide search with Search button, location on the right. Left column 640px on grey: chain share rail as proportional filter segments, a summary line, the yellow answer label with a ~96px price numeral, then sort controls and white labels on rails. Right: full-height map with label-style pins; the cheapest pin is yellow.

FORM: Shelf Edge, position 6 on the ordered list; seed:02887da7ea9e0781. Raises: hierarchy by scale alone (specimen), nothing labelled twice (catalog sleeve), share-proportional chain rail (acetate manual), pointing at a label lifts its pin and dims the rest (streaming wall).

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
