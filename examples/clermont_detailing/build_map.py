"""Inline grid + competitors + commune names into a standalone map.html."""
import json
import os
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
WORK = Path(os.environ.get("GEO_WORK", HERE.parents[1] / "tmp" / "geo"))
OUT = WORK / "out"
CACHE = OUT / "communes.json"


def communes(codes):
    if CACHE.exists():
        known = json.loads(CACHE.read_text())
    else:
        known = {}
    depts = {c[:2] for c in codes} - {c[:2] for c in known}
    for d in sorted(depts):
        url = f"https://geo.api.gouv.fr/departements/{d}/communes?fields=nom"
        with urllib.request.urlopen(url, timeout=30) as r:
            for c in json.loads(r.read()):
                known[c["code"]] = c["nom"]
    CACHE.write_text(json.dumps(known, ensure_ascii=False))
    return {c: known[c] for c in codes if c in known}


def main():
    grid = json.loads((OUT / "grid.geojson").read_text())
    comps = json.loads((OUT / "competitors.json").read_text())

    codes = {str(f["properties"]["com"]).split(",")[0] for f in grid["features"]}
    names = communes(sorted(codes))
    missing = codes - set(names)
    if missing:
        print(f"no commune name for {sorted(missing)[:5]}", file=sys.stderr)

    html = (HERE / "map_template.html").read_text()
    for tag, val in (
        ("/*__GRID__*/null", json.dumps(grid, separators=(",", ":"))),
        ("/*__COMPETITORS__*/null", json.dumps(comps, ensure_ascii=False, separators=(",", ":"))),
        ("/*__COMMUNES__*/null", json.dumps(names, ensure_ascii=False, separators=(",", ":"))),
        ("__KEY__", os.environ["GOOGLE_MAPS_KEY"]),
        ("__N_DETAIL__", str(sum(c["tier"] == "detail" for c in comps))),
        ("__N_WASH__", str(sum(c["tier"] == "wash" for c in comps))),
    ):
        assert tag in html, f"placeholder {tag} missing from template"
        html = html.replace(tag, val, 1)

    dst = OUT / "map.html"
    dst.write_text(html, encoding="utf-8")
    print(
        f"wrote {dst} ({dst.stat().st_size / 1e6:.1f} MB): "
        f"{len(grid['features'])} cells, {len(comps)} competitors, {len(names)} communes",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
