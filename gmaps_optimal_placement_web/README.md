# gmaps_optimal_placement_web

The map. A Leptos island over a directory of studies, and the axum server that feeds it.

```text
gmaps_optimal_placement serve studies/
        │
        ├─ GET /              shell — the Maps bootstrap, the CSS, the island marker
        ├─ GET /studies.json  every stem in the directory, and the ones the CLI prebuilt
        ├─ GET /payload/{stem}  the evaluated study, multi-MB, built once per stem
        ├─ GET /pkg/*         the client wasm
        └─ /api/*             server fns: the pin file under XDG_DATA_HOME,
                              the key bindings under XDG_CONFIG_HOME
```

A tab is one study's `Model` plus the controls as they stood when it was last left. Switching writes
the live signals into the tab being left and reads the next one into them; `press` and `unmet` are
recomputed rather than stored, because they are functions of λ and the tier weights. There is one
`google.maps` instance for the page's whole life — `cells`, `competitors` and `pins` each replace
what is there, so a switch needs no teardown.

Rust owns every number: recompute, colourise, the capture score, the top-N sweep, every report row
and the whole control panel. `map_core.js` owns `google.maps` and nothing else — the map instance,
the markers, and the canvas fill loop, which stays there because it needs the live projection every
frame. The contract between them is four arrays and an opacity.

Nothing throws across that boundary: under `panic=abort` a rejected promise reaching wasm kills the
app, so `map_core.js` returns a banner string instead.
