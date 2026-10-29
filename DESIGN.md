---
name: Matpris
description: Live grocery prices from the stores around you, hung as shelf-edge labels.
colors:
  steel: "#26292b"
  steel-edge: "#3a3e41"
  ground: "#e6e8e4"
  label: "#ffffff"
  yellow: "#ffd400"
  yellow-ink: "#4b4000"
  offer: "#d4001a"
  offer-ink: "#a30015"
  ink: "#1a1c1d"
  ink-2: "#4a4e50"
  rule: "#9a9e9c"
typography:
  price-hero:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "clamp(4rem, 7vw, 6rem)"
    fontWeight: 900
    lineHeight: 1
    letterSpacing: "-0.02em"
    fontFeature: "'tnum', 'lnum'"
    fontVariation: "'wdth' 62"
  price:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "2.875rem"
    fontWeight: 900
    lineHeight: 1
    letterSpacing: "-0.02em"
    fontFeature: "'tnum', 'lnum'"
    fontVariation: "'wdth' 62"
  display:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "clamp(2.5rem, 4.4vw, 3.5rem)"
    fontWeight: 900
    lineHeight: 0.95
    letterSpacing: "-0.01em"
    fontVariation: "'wdth' 68"
  wordmark:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "1.75rem"
    fontWeight: 900
    lineHeight: 1
    letterSpacing: "-0.01em"
    fontVariation: "'wdth' 70"
  headline:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "1.375rem"
    fontWeight: 800
    fontVariation: "'wdth' 80"
  title:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 700
    lineHeight: 1.25
  segment-count:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "1.625rem"
    fontWeight: 900
    lineHeight: 1
    letterSpacing: "-0.02em"
    fontFeature: "'tnum', 'lnum'"
    fontVariation: "'wdth' 62"
  unit-price:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 700
    fontFeature: "'tnum'"
    fontVariation: "'wdth' 85"
  body:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "1.0625rem"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 600
  button:
    fontFamily: "Archivo, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 800
    letterSpacing: "0.04em"
    fontVariation: "'wdth' 80"
rounded:
  tag: "2px"
  label: "3px"
spacing:
  seam: "3px"
  label-stack: "12px"
  label-pad: "18px"
  gutter: "28px"
  gutter-narrow: "16px"
components:
  search-input:
    backgroundColor: "{colors.label}"
    textColor: "{colors.ink}"
    rounded: "{rounded.label}"
    padding: "0 16px"
    height: "46px"
  button-search:
    backgroundColor: "{colors.label}"
    textColor: "{colors.ink}"
    typography: "{typography.button}"
    rounded: "{rounded.label}"
    padding: "0 22px"
    height: "46px"
  button-search-hover:
    backgroundColor: "{colors.ground}"
  chain-segment:
    backgroundColor: "{colors.label}"
    textColor: "{colors.ink}"
    typography: "{typography.segment-count}"
    padding: "8px 10px 9px"
    height: "62px"
  shelf-label:
    backgroundColor: "{colors.label}"
    textColor: "{colors.ink}"
    rounded: "{rounded.label}"
    padding: "14px 18px 16px"
  answer-label:
    backgroundColor: "{colors.yellow}"
    textColor: "{colors.ink}"
    rounded: "{rounded.label}"
    padding: "18px 22px 20px"
  sort-toggle:
    backgroundColor: "{colors.label}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.label}"
    padding: "7px 12px"
  sort-toggle-selected:
    backgroundColor: "{colors.steel}"
    textColor: "{colors.label}"
  suggestion-tag:
    backgroundColor: "{colors.label}"
    textColor: "{colors.ink}"
    rounded: "{rounded.label}"
    padding: "8px 16px 9px"
  deal-tag:
    backgroundColor: "{colors.offer}"
    textColor: "{colors.label}"
    rounded: "{rounded.tag}"
    padding: "2px 7px"
  sale-badge:
    backgroundColor: "{colors.offer}"
    textColor: "{colors.label}"
    rounded: "{rounded.tag}"
    padding: "2px 6px"
  store-pin:
    backgroundColor: "{colors.label}"
    rounded: "{rounded.label}"
    padding: "0 6px"
    height: "24px"
  store-pin-cheapest:
    backgroundColor: "{colors.yellow}"
    textColor: "{colors.ink}"
    rounded: "{rounded.label}"
    padding: "0 10px"
    height: "36px"
---

# Design System: Matpris

## Overview

**Creative North Star: "Shelf Edge"**

Every result is a shelf-edge price label hanging from a steel shelf rail, ranked by the unit price shoppers read in the store. The page is the aisle: a charcoal steel rail across the top carries the wordmark, search and location, the ground is the cool grey of a fluorescent-lit store, and each product is a white label with its own thin steel strip above it. The cheapest answer is the one yellow label on the shelf, and the map repeats it: one yellow pin among white ones.

The system is dense and practical, built for scanning prices. Hierarchy comes from scale alone. The price numeral is the loudest thing on every label, set in condensed black Archivo with raised öre the way a printed shelf tag sets it. Everything else (product name, brand and size, store, distance) stays at normal width and modest size. Nothing is labelled twice: the chain logo names the chain, so the store name beside it drops the chain prefix.

The build replaced a green card list and a pill search bar; that world is gone. Each label keeps a small product image at its left edge so shoppers can recognise the pack.

**Key Characteristics:**
- Charcoal steel rail and steel strips on a cool grey ground; white label stock.
- One yellow label per screen: the cheapest answer, echoed by its map pin.
- Offer red only for deals, sale and errors.
- Condensed black price numerals (62% width) with raised öre, trimmed to the figures.
- Square 3px corners and short soft drop shadows under labels.
- One motion: the "hang" settle as labels arrive.

## Colors

A near-neutral steel-and-grey store interior with two loud signal colours, each with exactly one job.

### Primary
- **Shelf Steel** (steel): the header rail, selected sort toggle, focus outline, selection highlight, map search-circle stroke and the device-less centre marker. The structural colour of the world and the only state colour.
- **Steel Edge** (steel-edge): the 4px bottom edge of the rail, the border between list and map, the steel strip gradient above each label, and the top strip of suggestion tags.

### Secondary
- **Label Yellow** (yellow): the cheapest answer label, the cheapest store's map pin, and the PRIS half of the wordmark. Nothing else.
- **Yellow Label Ink** (yellow-ink): secondary text on the yellow label only (brand and size, deal small print, "Best at each chain" heading and per-chain item prices).

### Tertiary
- **Offer Red** (offer): fills of the multi-buy deal tag and the Sale badge.
- **Offer Ink** (offer-ink): red text for errors and notices, struck-through former prices, and unavailable chain statuses ("busy, try again").

### Neutral
- **Fluorescent Grey** (ground): the page ground and the hover fill of white buttons in the rail.
- **Label Stock** (label): every label, input, segment, toggle and pin; also text on steel and red.
- **Price Ink** (ink): all primary text and price numerals.
- **Small Print** (ink-2): meta lines, counts, basis line, footer and hidden-chain text.
- **Rail Grey** (rule): outline of a hidden chain segment and the scrollbar thumb.

Chain colours (Willys, Hemköp, Lidl, ICA, Coop) belong to the chains, not to this palette. They are defined in `CHAINS` in `src/lib/types.ts` and appear only as a chain's identifying edge: the 5px top strip of its filter segment and the border and pointer of its map pins.

### Named Rules
**The One Yellow Label Rule.** Label yellow marks the cheapest answer: the answer label, the cheapest map pin, and the PRIS half of the wordmark. It is never a hover, focus or selection colour; state is shown in steel.

**The Offer Red Rule.** Offer red means a deal, a sale or an error. It never decorates, never marks a chain, and never marks the cheapest result.

## Typography

**Display Font:** Archivo (self-hosted variable font, width 62–125%, weight 400–900), with system-ui fallback
**Body Font:** Archivo at normal width
**Label/Mono Font:** none; tabular figures from Archivo

**Character:** One family doing two jobs through its width axis. Condensed black for prices and the few display lines, normal-width for everything a shopper reads as text.

### Hierarchy
- **Price hero** (900, clamp(4rem, 7vw, 6rem), 62% width; 4rem on phones): the price on the yellow answer label.
- **Price** (900, 2.875rem, 62% width; 2.375rem on phones): the price on every white label.
- **Display** (900, clamp(2.5rem, 4.4vw, 3.5rem), 68% width, 0.95 line height): the pre-search question only.
- **Wordmark** (900, 1.75rem, 70% width, uppercase): MAT in white, PRIS in label yellow, on the rail.
- **Headline** (800, 1.375rem, 80% width): section heads such as "Cheapest near you" and "All results".
- **Title** (700, 1rem, 1.25 line height): product names; 1.1875rem on the answer label.
- **Segment count** (900, 1.625rem, 62% width, tabular): the result count in each chain segment, set like a price so it reads at a glance.
- **Unit price** (700, 0.875rem, 85% width, tabular): the comparison price under each numeral; 1rem on the answer.
- **Body** (400, 1.0625rem, 1.5 line height, max 52ch): intro copy.
- **Label** (600–700, 0.8125rem): meta, store line, segments, toggles, counts.
- **Button** (800, 80% width, uppercase, 0.04em tracking): Search and Use this address only.

### Named Rules
**The Shelf Tag Numeral Rule.** Prices are set in 62%-width, weight 900 Archivo with tabular lining figures. Öre is raised as a superscript at 0.46em, top-aligned. The numeral box is trimmed to the figures with `text-box: trim-both cap alphabetic` (line-height 0.78 fallback) so the unit price sits close beneath it.

**The Width Carries Rank Rule.** Condensed width is reserved for prices, the wordmark, the display question and headlines. Product names and body copy stay at normal width.

## Layout

Desktop is a fixed app shell at full viewport height: the steel rail on top (13px by 28px padding, 4px steel-edge bottom), then a two-column grid of a scrolling list column (max 680px, 22px by 28px padding) and a full-height map filling the rest, separated by a 4px steel-edge border. The search field is capped at 640px.

At 900px and below the shell becomes one scrolling page with 16px gutters. The rail wraps: wordmark, then full-width search, then the location line. Content is ordered summary, map (46vh, min 280px, bleeding to the edges between 4px steel-edge bands), results, footer.

Rhythm is tight. Rail controls and chain segments sit 3px apart, like strips on one rail. Labels stack 12px below their steel strips with 10px between list items. Section heads get 26–34px of space above them.

## Elevation & Depth

Depth is physical and shallow: labels hang in front of the grey ground and cast short, soft drop shadows. Nothing floats far.

### Shadow Vocabulary
- **Hanging label** (`box-shadow: 0 3px 6px rgb(0 0 0 / 0.12)`): every white label at rest.
- **Lifted label** (`box-shadow: 0 8px 18px rgb(0 0 0 / 0.18)`): a label on hover or focus-within.
- **Answer label** (`box-shadow: 0 6px 14px rgb(0 0 0 / 0.16)`): the yellow answer.
- **Map pin** (`box-shadow: 0 2px 4px rgb(0 0 0 / 0.3)`), **cheapest pin** (`0 4px 10px rgb(0 0 0 / 0.35)`).
- **Tag** (`0 2px 4px rgb(0 0 0 / 0.12)`, `0 4px 10px rgb(0 0 0 / 0.16)` on hover) and **map overlay chip** (`0 2px 6px rgb(0 0 0 / 0.18)`).

### Named Rules
**The Soft Hang Rule.** Shadows are short, blurred and black at 12–35% opacity, always falling straight down. No hard offset shadows, no coloured glows.

## Shapes

Square forms with small 3px corners. A label is square on top where it meets its 12px steel strip and rounded 3px only at the bottom; the strip itself is rounded 2px at the top. Filter segments and suggestion tags follow the same strip-on-top logic with a 5px top border (the chain's colour or steel edge). Deal tags and Sale badges use 2px. The sort toggle is a joined pair with 3px outer corners and a 1.5px steel border. Map pins are small labels with a 6px pointer triangle in the chain's colour. A dashed 1.5px steel rule separates the answer from its "Best at each chain" list, and the map's search radius is a dashed steel circle.

## Components

### Buttons
- **Shape:** gently squared (3px).
- **Search:** white label stock on the steel rail, ink text, 46px tall, 22px side padding, uppercase condensed 800. Hover fills fluorescent grey. Disabled is a mid grey (#8d9193) with steel text.
- **Link buttons on the rail:** white, underlined 1px at a 3px offset, 2px on hover; optional 14px inline SVG icon.
- **Focus:** a 3px steel outline at 2px offset everywhere; white on the steel rail.

### Chain Share Rail (signature)
The chain filter is a share-proportional segment rail: one white segment per priced chain, at least 62px tall, with a 5px top strip in the chain's colour. Each segment is a small tag: the chain logo top-left (14px; Lidl 22px, City Gross 18px; shrunk to fit if a segment is narrower), and the result count at the bottom in the segment-count numeral; with no matches it says "none online" in small print instead of a 0, since a chain's website may simply not price the product. Each segment's flex-grow equals its number of results, so widths show each chain's share and animate (400ms, hang easing) as results stream in, but no segment gets narrower than its content (76px), so a low-count chain never clips its figure. On phones the rail runs three segments per row (six chains make two even rows) rather than shrinking logos past reading. In place of the count a segment shows a status at 0.75rem, 85% width, wrapping to a second line rather than being cut: "searching…" and "no store nearby" in small print, "busy, try again" and "unavailable" in offer ink. A hidden chain turns transparent with a 1.5px rail-grey outline, line-through text and a greyscale logo. Chains without online prices, if any, share one full-width segment under the rail with a 135° hatched grey fill (6px stripes) and "no online prices"; when every chain has prices it is not drawn.

### Shelf Labels
- **Corner Style:** square top, 3px bottom corners, hung under a 12px steel strip.
- **Background:** label stock; label yellow for the answer only.
- **Shadow Strategy:** hanging label, lifted on hover and focus (see Elevation).
- **Internal Padding:** 14px 18px 16px (answer 18px 22px 20px; 12–16px on phones).
- **Content:** product name, brand and size, then the chain logo with the store name (chain prefix dropped) and distance. A small product image (48px, 64px on the answer label, 40px and 48px on phones; an empty grey tile when there is none) sits at the left edge. The price column on the right holds an optional Sale badge, the numeral (always the figure the list is ranked by: price per kg/l when ranking by unit price, with a small /kg or /l suffix, else the item price), the other price under it, and the struck-through former price in offer ink. Loose fruit shows ≈ before an estimated piece price.
- **Answer:** adds "each when you buy N" for deals and, below a dashed rule, "Best at each chain" rows with a 64px logo column, a truncating name and a tabular price.
- **Motion:** labels arrive with the hang settle, translating from 6px above and rotated −0.5deg into place over 480ms on `cubic-bezier(0.16, 1, 0.3, 1)` from a top-left origin. Disabled under reduced motion.

### Chips
- **Deal tag:** offer-red fill, white condensed 800 text, 2px corners, reading "N for X kr", followed by the per-item price and small print.
- **Suggestion tags:** white labels with a 5px steel-edge top strip; on hover they drop 1px with a deeper shadow.

### Inputs / Fields
- **Style:** white field, no border, 3px corners, 46px tall (address field 40px), weight 600 text, regular-weight placeholder in #6a6e70.
- **Sort toggle:** a joined two-option control with a 1.5px steel border; the selected option fills steel with white text.
- **Checkbox:** native, 16px, tinted steel via `accent-color`.

### Map Pins
Pins are miniature shelf labels: white stock, 2px border and pointer in the chain's colour, chain logo inside. Chains without online prices are scaled to 85% at 75% opacity. The cheapest store's pin is the yellow label: 36px tall with its price in condensed black numerals, drawn on top. Pointing at a list label with a mouse lifts its pin (scale 1.2) and dims every other pin to 30%. Keyboard focus within a label does the same; touch does not.

## Do's and Don'ts

### Do:
- **Do** keep label yellow for the cheapest answer, its map pin and the PRIS half of the wordmark.
- **Do** show hover, focus, selection and pressed state in steel or with shadow, never in yellow.
- **Do** keep offer red for deals, sale and errors.
- **Do** set every price in 62%-width black Archivo with raised öre, trimmed with `text-box`.
- **Do** drop the chain prefix from a store name when the chain logo sits beside it.
- **Do** size chain filter segments by each chain's share of the results, and give chains without online prices the hatched segment.
- **Do** limit pin dimming on hover to mouse pointers (`pointerType === 'mouse'`) and keyboard focus.
- **Do** disable the hang settle and segment and tag transitions under `prefers-reduced-motion`.

### Don't:
- **Don't** let a product image grow past a small tile; the numeral stays the loudest thing on the label.
- **Don't** use label yellow as a hover, focus or selection colour.
- **Don't** use offer red for chain identity, decoration or the cheapest result.
- **Don't** use chain colours anywhere except as a chain's identifying edge on its segment and pins.
- **Don't** label anything twice.
- **Don't** add motion beyond the hang settle and the small state transitions on segments, tags, labels and pins.
