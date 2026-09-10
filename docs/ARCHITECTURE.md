# gmaps_optimal_placement

Where should a service business open. One Nix file describes an area, a statistical grid, what
counts as a competitor and how demand follows from whatever columns that grid publishes; the tool
serves a map that answers it.

The question is the input. Retargeting a city, a country or a trade is an edit to the study
document, never to the code.

```
              study.nix ─── the pinned interface
                   │
   ┌───────────────┴───────────────────────────────────────────┐
   │ gmaps_optimal_placement_sources        network, cache     │
   │   GridSource ─ INSEE Filosofi 200 m                       │
   │               GEOSTAT 1 km                                │
   │   PoiSource  ─ Google Places                              │
   │   probe      ─ the same, asked from a node                │
   │   SearchVolume ─ Google Ads · DataForSEO                  │
   └──────┬────────────────────────────────────┬───────────────┘
          │                                    │  the grid rolled up by commune
          │              ┌─────────────────────┴─────────────────────┐
          │              │ gmaps_optimal_placement_misc              │
          │              │   Country · Tally — what a cadastre draws,│
          │              │   per commune, ÷ any column, one map      │
          │              └───────────────────────────────────────────┘
          │  cells + named columns, POIs + tiers, orderings, keyword series
   ┌──────┴────────────────────────────────────────────────────┐
   │ gmaps_optimal_placement_core     no I/O, wasm-safe        │
   │   Reproject · CellId · Grid · Expr                        │
   │   rank — features, Plackett–Luce, COEF                    │
   │   Payload — the whole thin waist                          │
   │   model — pressure, unmet, capture, top-N                 │
   └───────┬───────────────────────────────────────┬───────────┘
           │  every expression                     │  every slider
   ┌───────┴──────────────────────────┐   ┌────────┴──────────────────────┐
   │ gmaps_optimal_placement          │   │ gmaps_optimal_placement_web   │
   │   CLI, study, HTML               │──▶│   ssr: axum + server fns      │
   └──────────────────────────────────┘   │   hydrate: the MapView island │
                                          │   map_core.js: google.maps    │
                                          └────────┬──────────────────────┘
                                                   ▼
                                    a served map · one <name>-searches.html
```

The map answers where the people are. It does not answer how many are looking for the thing, which
is what `searches` is for: a cell can be dense, affluent and uncontested and still sit under a trade
nobody searches for.

## The line between baked and live

Rust evaluates everything that a slider cannot move — the demand model and every configured layer —
into a `Payload` the browser fetches. Competitor pressure, underserved demand, the capture score and
the top-N sweep are recomputed in the page, because they are functions of λ and the tier weights,
which are live controls. Moving either side across this line costs the map its interactivity or the
study its reproducibility.

Competitor weight is baked, and deliberately: `rank` scores every competitor once in `build`, before
the payload is serialised, so the browser only re-weights by tier and λ. Whatever the scoring
function grows into, it never has to reach wasm. The one thing scored live is the what-if — one
business, one arithmetic pass.

Both sides are Rust. `gmaps_optimal_placement_core` is wasm-safe and holds the model, so the same code that
`cargo t` pins against a fixture is the code the browser runs.

## The line between Rust and JavaScript

`map_core.js` rides in as a wasm-bindgen snippet and is the only file that names `google.maps`: the
map instance, the markers and their info windows, and the canvas fill loop — which stays there
because it needs `fromLatLngToDivPixel` every frame. Rust hands it `ringX`/`ringY`, `colors`,
`shown` and an opacity, and owns every one of those numbers. The pattern is `v_utils::lwc`'s.

Nothing throws across that boundary. Under `panic=abort` a rejected promise reaching wasm kills the
app, so every entry point in `map_core.js` returns a banner string instead.

## Invariants

- **Reprojection is exact or the build fails.** A silently shifted grid is the one bug that looks
  fine and is entirely wrong. Reference points are pinned in `gmaps_optimal_placement_core::proj`.
- **No fallbacks on missing or malformed data.** A cell that will not parse is an error, never a
  zero. Imputed-versus-observed provenance survives to the map.
- **Config defines the model; code defines the mechanism.** Anything a study would want to vary
  belongs in the study file. Anything two studies share belongs in Rust. How Google ranks is the
  same mechanism for a plumber and a detailer, so the reviews→prominence curve is a fitted constant
  in `core::rank`, not an expression a study writes; what the study says is which queries it cares
  about and what each is worth.
- **A fitted quantity is refitted, never hand-edited.** `rank::COEF` is the output of
  `gmaps_optimal_placement fit` over the orderings in the work dir. Nudging a coefficient because the map looks
  wrong turns a measurement back into the guess it replaced.
- **The study file is a seed, never a sink.** `serve` only reads it. Pins promoted or hidden on the
  map are a diff beside it, under `XDG_DATA_HOME` — data, not cache, because a promoted candidate is
  a decision and cache is what cleaners delete.
- **`GOOGLE_MAPS_KEY` lives in the server's environment, never in an artifact.** Bulk archives and
  API responses cache under `GMAPS_OPTIMAL_PLACEMENT_WORK` (default `./tmp/geo`) so a rerun costs nothing — the
  INSEE archive is ~87 MB and Places calls are billed.

## Sources are an enum

The set of statistical sources is closed and known at compile time, so `GridSource` and `PoiSource`
are enums matched on, not traits implemented. Adding a country adds a variant and a row to the table
it matches on.

What the sources disagree about is which columns exist, so a grid carries columns discovered from
its archive rather than a struct per country. Expressions name those columns, and an expression
naming one that does not exist fails against the source's own column list before a single POI call
is made.

## Search-volume providers are a trait

The set of providers is not closed: they differ on price, geography and honesty, and a new one is a
purchase, not a country. So `searches::SearchVolume` is a trait, one file per implementation, and
`searches::provider` is the one `match` that knows the set — the same containment the enums give,
without pretending the set is compile-time known.

They agree on the string that names a place, because DataForSEO's `location_name` format *is*
Google's `canonicalName`. Switching provider is a one-word study edit.

Grouping is by essence, not by string: a study names seeds, the provider expands them semantically,
a regex filters that expansion and the survivors sum into one line. The expansion is fuzzy, so every
member and its own series survives into the page — a sum you cannot audit is a sum you cannot argue
with. A keyword the provider has no data for is excluded from the sum, never zero-filled.

## What a map like this cannot know

These are not caveats about the implementation. They are the distance between the model and the
decision, and they are the difference between a map that informs one and a map that launders a
guess into authority. The worked examples are in `examples/`.

- **Motorisation is estimated, not measured.** No 200 m source publishes households-with-a-car;
  INSEE has it only at IRIS level. The Clermont study infers it from the house/flat split
  (`men_mais × 1.55 + men_coll × 0.85`). Directionally right, not a measurement.
- **Most fine-grained cells are modelled.** ~80 % of INSEE 200 m cells have fewer than 11 fiscal
  households, so their values are imputed. The flag is carried through to the map and the "hide
  imputed cells" control exists for it. Trust dense cells, distrust isolated rural ones.
- **Residential demand only.** No traffic counts, no workplace population, no rent or premises
  availability. For a walk-in bay, passing traffic matters as much as who lives nearby.
- **A competitor sits at its registered address**, which understates a mobile operator's reach.
- **Tiering is name-based.** A shop whose name says nothing about what it does is tiered on what its
  name does say.
- **Every constant in a study is a guess with a sane magnitude**, not a fitted value. They are in
  the study file so they can be argued with. Competitor weight is the exception: it comes out of
  `rank`, and the three things below are what that estimate cannot settle.
- **Reviews cause rank and rank causes reviews.** A shop ranks well and is therefore seen, clicked
  and reviewed. The coefficient is co-movement, not causation, so the what-if reads "reviews
  associated with that rank" and never "reviews needed".
- **The Places API ordering is not the local pack** a customer sees in Maps. It correlates with it;
  it is a different list, from a different endpoint, with no personalisation and no map viewport.
- **Name relevance is observed after Google's own filter.** These results came back *because* they
  matched the query, so the variation among them is compressed and the name coefficient rests on the
  businesses that sat at the censoring boundary.
- **City-level search volumes are bucketed and small.** Keyword Planner rounds hard (0, 10, 20, 30,
  50, 70, 90, 110…) and an account with no campaign spend gets the coarsest treatment. For a niche
  trade in a 150k city, expect a line in the tens that moves in steps. The shape of the year is
  trustworthy; the level is an order of magnitude, not a count.
- **Summing an expansion overstates, and counts events not people.** Google already merges close
  variants — plurals, accents, misspellings — into one keyword, so members are distinct queries and
  the sum is defensible. But two phrasings of one intent still add, and a person who searches twice
  is two searches.
- **Search volume is not local demand.** It excludes everyone who finds a detailer through Maps
  without a query, through a friend, or by driving past.
