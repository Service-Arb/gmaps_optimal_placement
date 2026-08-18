# Clermont-Ferrand — car detailing site selection

Open **`out/map.html`** in a browser. Google Maps basemap, your competitors on it,
INSEE 200 m socio-economic grid painted over the top.

```
INSEE Filosofi 2021           Google Places API (New)
200 m grid, 34 vars           text search x 9 tiles x 8 queries
   |  build_grid.py              |  fetch_competitors.py
   |  bbox filter + 3035->4326   |  tier: detail / wash, drop retail noise
   v                             v
out/grid.geojson  ----.    .---- out/competitors.json
  10 446 cells        \  /        140 shops (46 detailing, 94 wash)
  455 k people         \/
                  build_map.py  (+ commune names, geo.api.gouv.fr)
                       |
                       v
                  out/map.html   3.5 MB, self-contained, no server needed
```

## What the map gives you

Layer dropdown, all on the same 200 m cells:

| Layer | Meaning |
|---|---|
| **Underserved demand** ★ | demand ÷ (1 + competition already reaching the cell) — the one to shop on |
| Demand proxy | est. cars × (standard of living / 22 k)^1.6 |
| Competitor pressure | Σ competitors of weight × exp(−distance/λ) |
| Population / Households / Est. cars / Households in houses | raw INSEE |
| Standard of living €/yr | per-person rate, **not** a density |

Live controls: catchment radius λ, how much a rollover wash counts against you,
opacity, marker filters, hide INSEE-imputed cells.

- **Click anywhere** → capture score, population and demand within 1 / 3 km,
  distance to nearest detailer, competitor counts within 2 / 5 km.
- **Rank top 10 sites** → Huff-style sweep over ~2 600 candidate cells, greedy
  pick kept ≥1.5 km apart.

At the defaults the ranking lands on **Chamalières, Royat, Ceyrat** (affluent,
house-heavy, west) and **Châtel-Guyon / Riom / Saint-Bonnet-près-Riom** (north,
thin competition). Downtown Clermont scores high on people and low on
opportunity — 6 detailers within 2 km already.

## Rebuild

```fish
nix-shell -p "python3.withPackages(ps: [ps.pyproj])" --run "python3 build_grid.py"
python3 fetch_competitors.py     # cached in data/places_cache, reruns are free
python3 build_map.py
```

`GOOGLE_MAPS_KEY` must be set (it is, in your env). The first competitor fetch
cost 89 Places text-search calls; the cache means editing the tiering rules
costs nothing.

Retarget another city by editing `LAT0/LAT1/LON0/LON1` in `build_grid.py` and
`fetch_competitors.py`, and the map `center` in `map_template.html`.

## Check

```fish
cd out; python3 -m http.server 8731 &
node smoke.js          # drives headless Chromium over CDP
```

Asserts the grid loads, all 140 markers attach, population totals 454 985,
pressure drops when washes are excluded and rises with λ, ranking produces 10
pins, every layer colours without throwing, and no uncaught exceptions.

## What the model does and does not know

- **Motorisation is estimated, not measured.** INSEE publishes households-with-a-car
  only at IRIS level; the 200 m grid has no such variable. Cars are inferred from
  the house/flat split (`men_mais × 1.55 + men_coll × 0.85`). Directionally right,
  not a measurement. Upgrading means joining IRIS `P21_RP_VOIT1P` — worth it only
  if a decision hinges on it.
- **80 % of 200 m cells are INSEE-imputed** (fewer than 11 fiscal households → the
  value is modelled, not observed). The `est` flag is carried through and the
  "hide imputed" checkbox exists for this reason. Trust dense cells, distrust
  isolated rural ones.
- **Residential demand only.** No traffic counts (TMJA), no workplace population,
  no rent or commercial-premises availability. For a walk-in bay, passing traffic
  matters as much as who lives nearby — that layer is not here.
- **Mobile detailers are pinned at their registered address**, which understates
  their real reach. Several of the 46 are mobile.
- **Competitor tiering is name-based.** A shop whose name says nothing about
  detailing but does it anyway is classed "wash".
- All constants (`CARS_PER_HOUSE`, `NV_REF`, `NV_ELAST`, review weighting) are at
  the top of the `<script>` in `map_template.html`. They are guesses with sane
  magnitudes, not fitted values.
