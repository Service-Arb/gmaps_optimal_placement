"""Competitor inventory from Google Places API (New).

Text Search caps at 60 results per query, so we tile the bbox and run several
French/English phrasings, then dedupe on place id. Raw responses are cached to
disk: reruns cost nothing.
"""
import hashlib
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).parent
CACHE = HERE / "data" / "places_cache"
OUT = HERE / "out" / "competitors.json"

KEY = os.environ["GOOGLE_MAPS_KEY"]
LAT0, LAT1 = 45.55, 45.95
LON0, LON1 = 2.90, 3.40
TILES = 3  # TILESxTILES rectangles over the bbox

FIELDS = ",".join(
    "places." + f
    for f in (
        "id displayName formattedAddress location rating userRatingCount types "
        "primaryType primaryTypeDisplayName businessStatus websiteUri "
        "nationalPhoneNumber regularOpeningHours.openNow"
    ).split()
)

QUERIES = [
    "lavage auto",
    "nettoyage voiture",
    "car detailing",
    "car wash",
    "station de lavage",
    "lavage auto sans eau",
    "esthétique automobile",
    "covering carrosserie",
]

import re

# A rollover wash at a hypermarket is weak competition for detailing; a
# dedicated detailer is direct. Text Search also drags in unrelated retail
# (DIY stores, clothes laundries) which is dropped entirely.
DETAIL_RE = re.compile(
    r"detail|esthétique|esthetique|nettoyage|clean|polissage|céramique|ceramique"
    r"|\bppf\b|covering|renovation auto|rénovation auto|carrosserie",
    re.I,
)
WASH_RE = re.compile(r"lavage|lav'|lav’|\blav\b|wash|karcher|kärcher|rouleau", re.I)
# "lav'"/"wash" also match clothes laundromats and a bike-wash point
NOT_CARS_RE = re.compile(r"laverie|vélo|velo|\bbike\b|pressing|blanchisserie", re.I)


def post(url, body):
    key = hashlib.sha1((url + json.dumps(body, sort_keys=True)).encode()).hexdigest()
    cached = CACHE / f"{key}.json"
    if cached.exists():
        return json.loads(cached.read_text())
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode(),
        headers={
            "Content-Type": "application/json",
            "X-Goog-Api-Key": KEY,
            "X-Goog-FieldMask": FIELDS + ",nextPageToken",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            out = json.loads(r.read())
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"{e.code} {e.read().decode()[:400]}") from e
    CACHE.mkdir(parents=True, exist_ok=True)
    cached.write_text(json.dumps(out))
    time.sleep(0.2)
    return out


def tiles():
    dlat = (LAT1 - LAT0) / TILES
    dlon = (LON1 - LON0) / TILES
    for i in range(TILES):
        for j in range(TILES):
            yield {
                "low": {"latitude": LAT0 + i * dlat, "longitude": LON0 + j * dlon},
                "high": {
                    "latitude": LAT0 + (i + 1) * dlat,
                    "longitude": LON0 + (j + 1) * dlon,
                },
            }


def main():
    found = {}
    calls = 0
    for rect in tiles():
        for q in QUERIES:
            token = None
            for _ in range(3):  # 3 pages x 20 = API maximum
                body = {
                    "textQuery": q,
                    "pageSize": 20,
                    "locationRestriction": {"rectangle": rect},
                }
                if token:
                    body["pageToken"] = token
                res = post("https://places.googleapis.com/v1/places:searchText", body)
                calls += 1
                for p in res.get("places", []):
                    found.setdefault(p["id"], p)
                token = res.get("nextPageToken")
                if not token:
                    break
        print(f"tile done: {len(found)} unique after {calls} calls", file=sys.stderr)

    out = []
    for p in found.values():
        if p.get("businessStatus") not in (None, "OPERATIONAL"):
            continue
        name = p.get("displayName", {}).get("text", "")
        types = p.get("types", [])
        ptype = p.get("primaryType", "")
        if ptype == "laundry" or NOT_CARS_RE.search(name):
            continue
        if DETAIL_RE.search(name):
            tier = "detail"
        elif ptype == "car_wash" or "car_wash" in types or WASH_RE.search(name):
            tier = "wash"
        else:
            continue
        out.append(
            {
                "id": p["id"],
                "name": name,
                "addr": p.get("formattedAddress", ""),
                "lat": p["location"]["latitude"],
                "lng": p["location"]["longitude"],
                "rating": p.get("rating"),
                "n_rev": p.get("userRatingCount", 0),
                "type": ptype,
                "type_fr": p.get("primaryTypeDisplayName", {}).get("text", ""),
                "web": p.get("websiteUri", ""),
                "tel": p.get("nationalPhoneNumber", ""),
                "tier": tier,
            }
        )
    out.sort(key=lambda r: -r["n_rev"])

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, ensure_ascii=False, indent=1))
    n_direct = sum(r["tier"] == "detail" for r in out)
    print(
        f"wrote {OUT}: {len(out)} kept ({n_direct} detailing, {len(out) - n_direct} wash),"
        f" {len(found)} raw, {calls} API calls",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
