# Clermont-Ferrand — car detailing site selection

The study is [`config.nix`](config.nix). There is no code here.

```fish
nix run .#study examples/clermont_detailing/config.nix
```

builds `tmp/geo/out/map.html` — Google Maps basemap, competitors on it, INSEE 200 m
socio-economic grid painted over the top — and then drives it in headless Chromium.

```
INSEE Filosofi 2021           Google Places API (New)
200 m grid, 32 columns        text search x 9 tiles x 8 queries
   |  bbox filter, 3035->4326    |  tier: detail / wash, drop unrelated retail
   v                             v
10 446 cells  ------------.  .---------- 140 shops (46 detailing, 94 wash)
455 k people               \/
                            map.html          1.3 MB, no server needed
```

## What the map gives you

| Layer | Meaning |
|---|---|
| **Underserved demand** ★ | demand ÷ (1 + competition already reaching the cell) — the one to shop on |
| Competitor pressure | Σ competitors of weight × exp(−distance/λ) |
| Demand | est. cars × (standard of living / 22 k)^1.6 |
| Population / Households / Est. cars / Households in houses | raw INSEE |
| Standard of living €/yr | per-person rate, **not** a density |

Live controls: catchment radius λ, the weight of one wash against one detailer, opacity, marker
filters per tier, hide imputed cells.

- **Click anywhere** → capture score, demand within 1 / 3 km, nearest competitor of each tier,
  counts within 2 / 5 km.
- **Compare candidates** → the `candidate` addresses side by side.
- **Rank top 10 sites** → Huff-style sweep over ~2 600 candidate cells, greedy pick kept ≥1.5 km
  apart.

At the defaults the ranking lands on **Chamalières, Châtel-Guyon, Ceyrat, Saint-Bonnet-près-Riom,
Riom, Royat** — affluent and house-heavy to the west, thin competition to the north. Downtown
Clermont scores high on people and low on opportunity: 6 detailers within 2 km already.

## Inputs

`GOOGLE_MAPS_KEY` must be set. The INSEE archive downloads once into `tmp/geo/data/` (~87 MB); the
first competitor fetch cost 89 Places text-search calls and is cached, so editing the tiering rules
costs nothing.

Retarget another city by copying `config.nix` and editing `area`. Retarget another trade by editing
`poi` and `model`.

## What this model does not know

See [docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md#what-a-map-like-this-cannot-know). Read it
before quoting a number from this map at anyone.
