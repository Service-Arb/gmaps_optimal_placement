CLI, study document and map rendering.

```sh
service_arb <study.nix> [-o map.html]
service_arb --schema            # JSON schema for the study document
```

The study is [`Study`]: a Nix file evaluating to an area, a grid source, what a competitor is, and
how demand follows from the columns that source publishes. `column` adds derived columns in list
order, so a study can name a quantity its source only implies. Every expression is evaluated before any of it
reaches the map — an expression naming a column that does not exist is an error, not a zero.

[`Payload`] is what the template receives. It carries evaluated per-cell values only: competitor
pressure, underserved demand, capture scores and the top-N sweep stay in JS, because they move with
the sliders.

`GOOGLE_MAPS_KEY` is read from the environment and embedded in the output, so the artifact stays
untracked. `SERVICE_ARB_WORK` (default `./tmp/geo`) is where archives and API responses cache.
