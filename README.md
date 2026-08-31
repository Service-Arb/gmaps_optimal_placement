# service_arb
![Minimum Supported Rust Version](https://img.shields.io/badge/nightly-1.92+-ab6000.svg)
![Lines Of Code](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/valeratrades/b48e6f02c61942200e7d1e3eeabf9bcb/raw/service_arb-loc.json)
<br>
[<img alt="ci errors" src="https://img.shields.io/github/actions/workflow/status/valeratrades/service_arb/errors.yml?branch=main&style=for-the-badge&style=flat-square&label=errors&labelColor=420d09" height="20">](https://github.com/valeratrades/service_arb/actions?query=branch%3Amain) <!--NB: Won't find it if repo is private-->
[<img alt="ci warnings" src="https://img.shields.io/github/actions/workflow/status/valeratrades/service_arb/warnings.yml?branch=main&style=for-the-badge&style=flat-square&label=warnings&labelColor=d16002" height="20">](https://github.com/valeratrades/service_arb/actions?query=branch%3Amain) <!--NB: Won't find it if repo is private-->

Where should a service business open?

One Nix file describes an area, a statistical grid, what counts as a competitor, and how demand
follows from the columns that grid publishes. `service_arb` paints that demand under the
competitors already on the ground and writes one self-contained HTML map.

The same file can name groups of queries, and `service_arb searches` charts how often each group is
asked for over the trailing year — the map says where the people are, this says how many of them
are looking for the thing.

The question is the input: a different city, country or trade is an edit to the study document, not
to the code.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the invariants, and
[examples/clermont_detailing](examples/clermont_detailing) for a worked study.
<!-- markdownlint-disable -->
<details>
<summary>
<h2>Installation</h2>
</summary>

```sh
cargo install --path service_arb
```

</details>
<!-- markdownlint-restore -->

## Usage
```sh
## Set your Google Maps API key. The map embeds it.
export GOOGLE_MAPS_KEY=...

## Write the map. Bulk archives and API responses cache in $SERVICE_ARB_WORK (default ./tmp/geo).
service_arb map examples/clermont_detailing/config.nix

## Chart the monthly search volume of each query group. Set the credentials of the provider first:
## GOOGLE_ADS_{DEVELOPER_TOKEN,CLIENT_ID,CLIENT_SECRET,REFRESH_TOKEN,CUSTOMER_ID}, or
## DATAFORSEO_{LOGIN,PASSWORD}.
service_arb searches examples/clermont_detailing/config.nix

## Show the schema of the study document.
service_arb schema
```

## Sources

| `grid.source` | Area | Cell | Publishes |
|---|---|---|---|
| `insee_filosofi_200m` | France | 200 m | households, housing type, standard of living |
| `geostat_1km` | EU | 1 km | census counts by age, sex, employment, origin |

| `searches.provider` | Credentials | Notes |
|---|---|---|
| `google_ads` | `GOOGLE_ADS_*` | Keyword Planner at the source; needs Basic API access |
| `dataforseo` | `DATAFORSEO_LOGIN`, `DATAFORSEO_PASSWORD` | the same numbers, resold per call |

## Expressions

`column`, `model` and `layer` are arithmetic over the column names the chosen source publishes,
plus `max`, `min`, `clamp`, `sqrt` and `pow`. A name that does not exist is an error at load,
against the source's own column list.


<br>

<sup>
	This repository follows <a href="https://github.com/valeratrades/.github/tree/master/best_practices">my best practices</a> and <a href="https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md">Tiger Style</a> (except "proper capitalization for acronyms": (VsrState, not VSRState) and formatting). For project's architecture, see <a href="./docs/ARCHITECTURE.md">ARCHITECTURE.md</a>.
</sup>

#### License

<sup>
	Licensed under <a href="LICENSE">Blue Oak 1.0.0</a>
</sup>

<br>

<sub>
	Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be licensed as above, without any additional terms or conditions.
</sub>

