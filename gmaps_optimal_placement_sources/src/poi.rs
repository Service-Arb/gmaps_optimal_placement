//! Competitor inventory: whoever is already selling the service in the area.
use eyre::{Result, WrapErr, bail, ensure};
use gmaps_optimal_placement_core::{grid::Bbox, payload::Poi};
use indexmap::IndexMap;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::work::Work;

pub const SEARCH_TEXT: &str = "https://places.googleapis.com/v1/places:searchText";
/// Enterprise SKU: `rating`, `websiteUri` and `nationalPhoneNumber` each pull the mask up a tier.
/// The map displays all three.
const FIELDS: &str = "places.id,places.displayName,places.formattedAddress,places.location,places.rating,places.userRatingCount,\
	places.types,places.primaryType,places.primaryTypeDisplayName,places.businessStatus,places.websiteUri,places.nationalPhoneNumber,nextPageToken";
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub enum PoiSource {
	#[serde(rename = "google_places")]
	GooglePlaces,
}

/// A name pattern, a set of source-native categories, or both — either one hits.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatchSpec {
	/// Case-insensitive regex over the display name.
	#[serde(default, rename = "match")]
	pub pattern: Option<String>,
	#[serde(default)]
	pub types: Vec<String>,
}
impl MatchSpec {
	fn compile(&self) -> Result<Matcher> {
		ensure!(self.pattern.is_some() || !self.types.is_empty(), "a match rule needs `match`, `types`, or both");
		let pattern = self
			.pattern
			.as_deref()
			.map(|p| Regex::new(&format!("(?i){p}")))
			.transpose()
			.wrap_err_with(|| format!("match pattern {:?}", self.pattern.as_deref().unwrap_or_default()))?;
		Ok(Matcher { pattern, types: self.types.clone() })
	}
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tier {
	pub name: String,
	/// How much one of these counts against a new entrant, relative to a direct competitor.
	pub weight: f64,
	#[serde(flatten)]
	pub spec: MatchSpec,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PoiConfig {
	pub source: PoiSource,
	pub queries: Vec<String>,
	/// The bbox is searched as `tiles` x `tiles` rectangles: one text search returns at most 60.
	pub tiles: u32,
	/// First match wins; anything matching none is not a competitor. `types` here hits any category
	/// the source lists — a supermarket forecourt is typed `gas_station` and carries `car_wash`.
	#[serde(rename = "tier")]
	pub tiers: Vec<Tier>,
	/// Checked before the tiers — text search drags in unrelated retail. `types` here hits only the
	/// source's *primary* category, because the full list is noisy enough to veto real competitors:
	/// Google tags several car washes `laundry`.
	pub drop: Option<MatchSpec>,
}

/// What Places returned for one query, in the order it ranked them. Both the tiled inventory sweep
/// and the probe produce these; the fit reads nothing else.
#[derive(Clone, Debug)]
pub struct Ranking {
	pub from: Region,
	pub term: String,
	pub ids: Vec<String>,
}

/// Where the search was asked from, which is what says who could have been returned.
#[derive(Clone, Copy, Debug)]
pub enum Region {
	/// `locationRestriction`: a hard filter with nobody standing in it. Distance is not defined,
	/// and a coefficient fitted on it would be measuring the tiling.
	Tile(Bbox),
	/// `locationBias`: a searcher at [lat, lon], and a radius the ranking may reach past.
	Node([f64; 2], f64),
}

/// The competitors, and every ordering the search that found them handed back.
pub struct Inventory {
	pub pois: Vec<Poi>,
	pub obs: Vec<Ranking>,
}

/// One text search, paged, in rank order. The body and the field mask are the caller's, because
/// they are what decides the SKU.
pub fn search_text(work: &Work, key: &str, body: &serde_json::Value, mask: &str, pages: usize) -> Result<Vec<(String, serde_json::Value)>> {
	let mut out = Vec::new();
	let mut token: Option<String> = None;
	for _ in 0..pages {
		let mut body = body.clone();
		if let Some(t) = &token {
			body["pageToken"] = serde_json::Value::String(t.clone());
		}
		let res = work.cached_post(SEARCH_TEXT, &body, &[("X-Goog-Api-Key", key), ("X-Goog-FieldMask", mask)])?;
		if let Some(e) = res.get("error") {
			bail!("Places text search {}: {e}", body["textQuery"]);
		}
		for p in res["places"].as_array().into_iter().flatten() {
			let id = p["id"].as_str().ok_or_else(|| eyre::eyre!("Places returned a result without an id: {p}"))?;
			out.push((id.to_owned(), p.clone()));
		}
		match res["nextPageToken"].as_str() {
			Some(t) => token = Some(t.to_owned()),
			None => break,
		}
	}
	Ok(out)
}

pub fn load(cfg: &PoiConfig, bbox: Bbox, work: &Work) -> Result<Inventory> {
	let PoiSource::GooglePlaces = cfg.source;
	let key = std::env::var("GOOGLE_MAPS_KEY").wrap_err("GOOGLE_MAPS_KEY is not set")?;
	ensure!(cfg.tiles > 0, "poi.tiles must be at least 1");
	let drop = cfg.drop.as_ref().map(MatchSpec::compile).transpose()?;
	let tiers: Vec<(&Tier, Matcher)> = cfg.tiers.iter().map(|t| Ok((t, t.spec.compile()?))).collect::<Result<_>>()?;
	ensure!(!tiers.is_empty(), "at least one [[poi.tier]] is needed to say what a competitor is");

	let mut raw: IndexMap<String, serde_json::Value> = IndexMap::new();
	let mut obs = Vec::new();
	let (dlat, dlon) = ((bbox.lat[1] - bbox.lat[0]) / cfg.tiles as f64, (bbox.lon[1] - bbox.lon[0]) / cfg.tiles as f64);
	for i in 0..cfg.tiles {
		for j in 0..cfg.tiles {
			// the cache key is the body verbatim, so these expressions may not be re-associated
			let (lat0, lon0) = (bbox.lat[0] + i as f64 * dlat, bbox.lon[0] + j as f64 * dlon);
			let (lat1, lon1) = (bbox.lat[0] + (i + 1) as f64 * dlat, bbox.lon[0] + (j + 1) as f64 * dlon);
			let rect = serde_json::json!({
				"low":  {"latitude": lat0, "longitude": lon0},
				"high": {"latitude": lat1, "longitude": lon1},
			});
			let tile = Bbox {
				lat: [lat0, lat1],
				lon: [lon0, lon1],
			};
			for q in &cfg.queries {
				let body = serde_json::json!({"textQuery": q, "pageSize": 20, "locationRestriction": {"rectangle": rect}});
				// 3 pages x 20 is the API maximum for one text search
				let page = search_text(work, &key, &body, FIELDS, 3)?;
				obs.push(Ranking {
					from: Region::Tile(tile),
					term: q.clone(),
					ids: page.iter().map(|(id, _)| id.clone()).collect(),
				});
				for (id, p) in page {
					raw.entry(id).or_insert(p);
				}
			}
		}
	}

	let mut out = Vec::new();
	for (id, p) in &raw {
		match p["businessStatus"].as_str() {
			None | Some("OPERATIONAL") => {}
			Some(_) => continue,
		}
		let Some(name) = p["displayName"]["text"].as_str() else {
			bail!("Places result {id} has no display name: {p}")
		};
		let primary = p["primaryType"].as_str().unwrap_or_default();
		let mut kinds: Vec<&str> = p["types"].as_array().into_iter().flatten().filter_map(serde_json::Value::as_str).collect();
		kinds.push(primary);
		if drop.as_ref().is_some_and(|d| d.hits(name, &[primary])) {
			continue;
		}
		let Some((tier, _)) = tiers.iter().find(|(_, m)| m.hits(name, &kinds)) else { continue };
		let (lat, lng) = (p["location"]["latitude"].as_f64(), p["location"]["longitude"].as_f64());
		let (Some(lat), Some(lng)) = (lat, lng) else {
			bail!("Places result {name:?} has no location: {p}")
		};
		out.push(Poi {
			id: id.clone(),
			name: name.to_owned(),
			addr: p["formattedAddress"].as_str().unwrap_or_default().to_owned(),
			lat,
			lng,
			rating: p["rating"].as_f64(),
			n_rev: p["userRatingCount"].as_f64().unwrap_or(0.), // Places omits the count at zero reviews
			kind: primary.to_owned(),
			kind_label: p["primaryTypeDisplayName"]["text"].as_str().unwrap_or_default().to_owned(),
			web: p["websiteUri"].as_str().unwrap_or_default().to_owned(),
			tel: p["nationalPhoneNumber"].as_str().unwrap_or_default().to_owned(),
			tier: tier.name.clone(),
		});
	}
	out.sort_by(|a, b| b.n_rev.total_cmp(&a.n_rev));
	eprintln!("places: {} raw over {} billed calls, {} kept", raw.len(), work.billed(), out.len());
	Ok(Inventory { pois: out, obs })
}
struct Matcher {
	pattern: Option<Regex>,
	types: Vec<String>,
}

impl Matcher {
	fn hits(&self, name: &str, kinds: &[&str]) -> bool {
		self.pattern.as_ref().is_some_and(|r| r.is_match(name)) || kinds.iter().any(|k| self.types.iter().any(|t| t == k))
	}
}
