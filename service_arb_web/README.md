# service_arb_web

The map. A Leptos island over a study's payload, and the axum server that feeds it.

```
service_arb serve study.nix
        │
        ├─ GET /            shell — the Maps bootstrap, the CSS, the island marker
        ├─ GET /payload.json  the evaluated study, multi-MB, fetched not baked
        ├─ GET /pkg/*         the client wasm
        └─ /api/*             server fns: the pin file under XDG_DATA_HOME
```

Rust owns every number: recompute, colourise, the capture score, the top-N sweep, every report row
and the whole control panel. `map_core.js` owns `google.maps` and nothing else — the map instance,
the markers, and the canvas fill loop, which stays there because it needs the live projection every
frame. The contract between them is four arrays and an opacity.

Nothing throws across that boundary: under `panic=abort` a rejected promise reaching wasm kills the
app, so `map_core.js` returns a banner string instead.
