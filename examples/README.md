# Worked studies

There is no code here. One file per `<trade>_-_<city>`, and a leading-underscore file per city
holding what every trade in it shares — frame, grid, raw INSEE layers, the location search volume
resolves against — pulled in with `import`.

```
              _Clermont-Ferrand.nix
        ^               ^               ^
        |               |               |
car_detailing_-_   plumbing_-_    cleaning_-_Clermont-Ferrand.nix
   poi / column / model / searches.group are the trade
```

```fish
nix run .#open examples/plumbing_-_Clermont-Ferrand.nix   # serve the map
nix run .#searches examples/plumbing_-_Clermont-Ferrand.nix   # monthly volume per query group
nix run .#study                                           # the detailing map, asserted in headless Chromium
```

## What the map gives you

| Layer | Meaning |
|---|---|
| **Underserved demand** ★ | demand ÷ (1 + competition already reaching the cell) — the one to shop on |
| Competitor pressure | Σ competitors of weight × exp(−distance/λ) |
| Demand | the study's `model.demand` |
| Population / Households / Households in houses | raw INSEE |
| Standard of living €/yr | per-person rate, **not** a density |

Live controls: catchment radius λ, the weight of one tier against the other, opacity, marker filters
per tier, hide imputed cells.

- **Click anywhere** → capture score, demand within 1 / 3 km, nearest competitor of each tier,
  counts within 2 / 5 km.
- **Compare candidates** → the `candidate` addresses side by side.
- **Rank top 10 sites** → Huff-style sweep over ~2 600 candidate cells, greedy pick kept ≥1.5 km
  apart.

For car detailing at the defaults the ranking lands on **Chamalières, Châtel-Guyon, Ceyrat,
Saint-Bonnet-près-Riom, Riom, Royat** — affluent and house-heavy to the west, thin competition to
the north. Downtown Clermont scores high on people and low on opportunity: 6 detailers within 2 km
already.

```
INSEE Filosofi 2021           Google Places API (New)
200 m grid, 32 columns        text search x 9 tiles x 8 queries
   |  bbox filter, 3035->4326    |  tier + drop
   v                             v
10 446 cells  ------------.  .---------- 140 shops (detailing: 46 detail, 94 wash)
455 k people               \/
                            map.html          1.3 MB, no server needed
```

## Inputs

`GOOGLE_MAPS_KEY` must be set. The INSEE archive downloads once into `tmp/geo/data/` (~87 MB) and is
shared by every study over the same country; Places responses are cached per query, so editing the
tiering rules costs nothing and a second trade only pays for its own queries.

## What these models do not know

See [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md#what-a-map-like-this-cannot-know). Read it before
quoting a number from one of these maps at anyone.
