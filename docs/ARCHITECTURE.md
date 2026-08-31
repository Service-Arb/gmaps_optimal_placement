# service_arb

Where should a service business open. One Nix file describes an area, a statistical grid, what
counts as a competitor and how demand follows from whatever columns that grid publishes; the tool
emits one self-contained HTML map that answers it.

The question is the input. Retargeting a city, a country or a trade is an edit to the study
document, never to the code.

```
              study.nix ─── the pinned interface
                   │
   ┌───────────────┴───────────────────────────────┐
   │ service_arb_sources        network, cache     │
   │   GridSource ─ INSEE Filosofi 200 m           │
   │               GEOSTAT 1 km                    │
   │   PoiSource  ─ Google Places                  │
   └───────────────┬───────────────────────────────┘
                   │  cells + named columns, POIs + tiers
   ┌───────────────┴───────────────────────────────┐
   │ service_arb_core           no I/O             │
   │   Reproject · CellId · Grid · Expr            │
   └───────────────┬───────────────────────────────┘
                   │  every expression evaluated
   ┌───────────────┴───────────────────────────────┐
   │ service_arb                CLI, config, HTML  │
   └───────────────┬───────────────────────────────┘
                   ▼
              one map.html
```

## The line between baked and live

Rust evaluates everything that a slider cannot move: the demand model and every configured layer,
baked into the page as arrays. Competitor pressure, underserved demand, the capture score and the
top-N sweep stay in JavaScript, because they are functions of λ and the tier weights, which are
live controls. Moving either side across this line costs the map its interactivity or the study its
reproducibility.

## Invariants

- **Reprojection is exact or the build fails.** A silently shifted grid is the one bug that looks
  fine and is entirely wrong. Reference points are pinned in `service_arb_core::proj`.
- **No fallbacks on missing or malformed data.** A cell that will not parse is an error, never a
  zero. Imputed-versus-observed provenance survives to the map.
- **Config defines the model; code defines the mechanism.** Anything a study would want to vary
  belongs in the study file. Anything two studies share belongs in Rust.
- **The output is one file** that opens from disk with no server.
- **The generated map embeds `GOOGLE_MAPS_KEY`; artifacts stay untracked.** Bulk archives and API
  responses cache under `SERVICE_ARB_WORK` (default `./tmp/geo`) so a rerun costs nothing — the
  INSEE archive is ~87 MB and Places calls are billed.

## Sources are an enum

The set of statistical sources is closed and known at compile time, so `GridSource` and `PoiSource`
are enums matched on, not traits implemented. Adding a country adds a variant and a row to the table
it matches on.

What the sources disagree about is which columns exist, so a grid carries columns discovered from
its archive rather than a struct per country. Expressions name those columns, and an expression
naming one that does not exist fails against the source's own column list before a single POI call
is made.

## What a map like this cannot know

These are not caveats about the implementation. They are the distance between the model and the
decision, and they are the difference between a map that informs one and a map that launders a
guess into authority. The worked example is `examples/clermont_detailing`.

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
  the study file so they can be argued with.
