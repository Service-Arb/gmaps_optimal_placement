"""INSEE Filosofi 2021 200m grid -> local GeoJSON for one bbox.

idcar_200m encodes the cell SW corner in EPSG:3035 (LAEA), so filtering is a
string parse; only surviving cells get reprojected.
"""
import csv
import io
import json
import sys
import zipfile
from pathlib import Path

from pyproj import Transformer

HERE = Path(__file__).parent
ZIP = HERE / "data" / "filosofi2021_200m_csv.zip"
MEMBER = "carreaux_200m_met.csv"
OUT = HERE / "out" / "grid.geojson"

# Clermont-Ferrand agglomeration, generous: Riom (N) to Issoire-ward (S),
# Volvic (W) to Lezoux-ward (E).
LAT0, LAT1 = 45.55, 45.95
LON0, LON1 = 2.90, 3.40

CELL = 200
fwd = Transformer.from_crs(4326, 3035, always_xy=True)
inv = Transformer.from_crs(3035, 4326, always_xy=True)


def laea_bbox():
    xs, ys = [], []
    for lon in (LON0, LON1):
        for lat in (LAT0, LAT1):
            x, y = fwd.transform(lon, lat)
            xs.append(x)
            ys.append(y)
    return min(xs), min(ys), max(xs), max(ys)


def main():
    x0, y0, x1, y1 = laea_bbox()
    print(f"LAEA bbox E {x0:.0f}..{x1:.0f}  N {y0:.0f}..{y1:.0f}", file=sys.stderr)

    rows = []
    with zipfile.ZipFile(ZIP) as z, z.open(MEMBER) as raw:
        f = io.TextIOWrapper(raw, encoding="utf-8", newline="")
        header = f.readline().rstrip("\r\n").split(",")
        col = {name: i for i, name in enumerate(header)}
        for n, line in enumerate(f):
            # "CRS3035RES200mN2029400E4259000": N at 15:22, E at 23:30 (fixed width)
            if n == 0:
                assert line[:15] == "CRS3035RES200mN" and line[22] == "E", line[:31]
            north = int(line[15:22])
            if not (y0 <= north <= y1):
                continue
            east = int(line[23:30])
            if not (x0 <= east <= x1):
                continue
            rows.append((east, north, line))
        print(f"scanned {n + 1} cells, kept {len(rows)}", file=sys.stderr)

    # lcog_geo is quoted and may hold several comma-separated commune codes
    parsed = list(csv.reader(r[2] for r in rows))
    rows = [(e, n, p) for (e, n, _), p in zip(rows, parsed)]

    num = lambda p, k: float(p[col[k]])
    feats = []
    for east, north, p in rows:
        corners = [
            (east, north),
            (east + CELL, north),
            (east + CELL, north + CELL),
            (east, north + CELL),
        ]
        ring = [
            [round(lon, 5), round(lat, 5)]
            for lon, lat in (inv.transform(x, y) for x, y in corners)
        ]
        ring.append(ring[0])

        ind = num(p, "ind")
        men = num(p, "men")
        snv = num(p, "ind_snv")
        mais = num(p, "men_mais")
        coll = num(p, "men_coll")
        feats.append(
            {
                "type": "Feature",
                "geometry": {"type": "Polygon", "coordinates": [ring]},
                "properties": {
                    "id": p[col["idcar_200m"]][14:],
                    "com": p[col["lcog_geo"]],
                    "ind": round(ind, 1),
                    "men": round(men, 1),
                    # ind_snv is the SUM of standard-of-living over individuals
                    "nv": round(snv / ind, 0) if ind else 0.0,
                    "mais": round(mais, 1),
                    "coll": round(coll, 1),
                    "pauv": round(num(p, "men_pauv"), 1),
                    "prop": round(num(p, "men_prop"), 1),
                    "surf": round(num(p, "men_surf"), 1),
                    "a25_54": round(num(p, "ind_25_39") + num(p, "ind_40_54"), 1),
                    "est": int(p[col["i_est_200"]]),  # 1 = imputed, not observed
                },
            }
        )

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({"type": "FeatureCollection", "features": feats}))
    tot = sum(f["properties"]["ind"] for f in feats)
    print(f"wrote {OUT} ({OUT.stat().st_size / 1e6:.1f} MB), pop {tot:,.0f}", file=sys.stderr)


if __name__ == "__main__":
    main()
