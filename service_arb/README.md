CLI and the study document.

```sh
service_arb serve    <study.nix> [--port 8731] [--open]
service_arb searches <study.nix> [-o searches.html]
service_arb schema                 # JSON schema for the study document
```

The study is [`Study`]: a Nix file evaluating to an area, a grid source, what a competitor is, and
how demand follows from the columns that source publishes. `column` adds derived columns in list
order, so a study can name a quantity its source only implies. Every expression is evaluated before
any of it reaches the map — an expression naming a column that does not exist is an error, not a
zero.

An optional `searches` block names groups of queries. [`fold`] is the half of that command that is
ours: the provider expands seeds semantically, a regex keeps or drops each member, and the survivors
sum into one line of [`SearchPayload`] — with their own series alongside, so the sum can be audited.

[`Payload`] is what `serve` hands the browser. It carries evaluated per-cell values only: competitor
pressure, underserved demand, capture scores and the top-N sweep are `service_arb_core::model`,
recomputed in the page because they move with the sliders.

`GOOGLE_MAPS_KEY` is read from the environment by the server and never written to disk.
`SERVICE_ARB_WORK` (default `./tmp/geo`) is where archives and API responses cache.
