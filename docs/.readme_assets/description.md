Where should a service business open?

Two Nix files describe it. A location gives an area and the statistical grid behind it; a trade —
applied to that location — gives what counts as a competitor and how demand follows from the columns
the grid publishes. `gmaps_optimal_placement` paints that demand under the competitors already on the
ground and serves the map. How much any one competitor counts is fitted, not guessed:
`gmaps_optimal_placement probe` asks Google the study's own queries from equal-demand points across the area,
and `gmaps_optimal_placement fit` estimates what its ordering rewards.

A trade also names groups of queries, and `gmaps_optimal_placement searches` charts how often each group is
asked for over the trailing year — the map says where the people are, this says how many of them
are looking for the thing.

The question is the input: another city is a location file, another trade is a trade file, and every
pairing of the two is a study. Neither is an edit to the code.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the invariants, and
[examples/](examples) for worked studies.
