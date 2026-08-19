Everything that reaches the network. Two closed enums, [`GridSource`] and [`PoiSource`]; adding a
country means adding a variant and a row to the table it matches on, not a trait implementation.

- [`grid::load`] returns a [`service_arb_core::Grid`] whose columns are whatever the chosen archive
  publishes. The bbox filter reads the cell corner out of the id, so only surviving cells are ever
  reprojected.
- [`poi::load`] returns competitors already sorted into the study's tiers. Tiering is the study's,
  not ours: [`PoiConfig`] is deserialized straight from its TOML.
- [`Work`] is the untracked cache. Bulk archives download once, POST responses are keyed by the
  request, so editing a query refetches and rerunning does not.

`GOOGLE_MAPS_KEY` must be in the environment for [`poi::load`].
