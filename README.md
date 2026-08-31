Where should a service business open?

One TOML file describes an area, a statistical grid, what counts as a competitor, and how demand
follows from the columns that grid publishes. `service_arb` paints that demand under the
competitors already on the ground and writes one self-contained HTML map.

The question is the input: a different city, country or trade is an edit to the study document, not
to the code.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the invariants, and
[examples/clermont_detailing](examples/clermont_detailing) for a worked study.

## Installation
```sh
cargo install --path service_arb
```

## Usage
```sh
# Set your Google Maps API key. The map embeds it.
export GOOGLE_MAPS_KEY=...

# Write the map. Bulk archives and API responses cache in $SERVICE_ARB_WORK (default ./tmp/geo).
service_arb examples/clermont_detailing/config.nix

# Show the schema of the study document.
service_arb --schema
```

## Sources

| `grid.source` | Area | Cell | Publishes |
|---|---|---|---|
| `insee_filosofi_200m` | France | 200 m | households, housing type, standard of living |
| `geostat_1km` | EU | 1 km | census counts by age, sex, employment, origin |

## Expressions

`[columns]`, `[model]` and `[[layer]]` are arithmetic over the column names the chosen source
publishes, plus `max`, `min`, `clamp`, `sqrt` and `pow`. A name that does not exist is an error at
load, against the source's own column list.