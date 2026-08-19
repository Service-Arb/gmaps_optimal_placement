Grid geometry, reprojection and the expression language. No network, no filesystem.

- [`Reproject`] — EPSG:3035 <-> WGS84. Reference points are pinned in its tests; nothing downstream
  is trustworthy without them.
- [`CellId`] — `CRS3035RES{res}mN{north}E{east}`, one parser across resolutions, and the cell ring
  it implies.
- [`Grid`] — cells plus `IndexMap<String, Vec<f64>>` of whatever the source published. Sources fill
  it through [`Grid::push`], which fixes the column set from the first row and rejects any row that
  disagrees.
- [`Expr`] — the arithmetic a study writes in TOML, over those column names. The callable surface is
  `max`, `min`, `clamp`, `sqrt`, `pow` and nothing else.
