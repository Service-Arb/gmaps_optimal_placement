//! Competitor inventory: whoever is already selling the service in the area.
use eyre::{Result, WrapErr, bail, ensure};
use indexmap::IndexMap;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_arb_core::{grid::Bbox, payload::Poi};

use crate::work::Work;

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

pub fn load(cfg: &PoiConfig, bbox: Bbox, work: &Work) -> Result<Vec<Poi>> {
	let PoiSource::GooglePlaces = cfg.source;
	let key = std::env::var("GOOGLE_MAPS_KEY").wrap_err("GOOGLE_MAPS_KEY is not set")?;
	ensure!(cfg.tiles > 0, "poi.tiles must be at least 1");
	let drop = cfg.drop.as_ref().map(MatchSpec::compile).transpose()?;
	let tiers: Vec<(&Tier, Matcher)> = cfg.tiers.iter().map(|t| Ok((t, t.spec.compile()?))).collect::<Result<_>>()?;
	ensure!(!tiers.is_empty(), "at least one [[poi.tier]] is needed to say what a competitor is");

	let mut raw: IndexMap<String, serde_json::Value> = IndexMap::new();
	let (dlat, dlon) = ((bbox.lat[1] - bbox.lat[0]) / cfg.tiles as f64, (bbox.lon[1] - bbox.lon[0]) / cfg.tiles as f64);
	let mut calls = 0;
	for i in 0..cfg.tiles {
		for j in 0..cfg.tiles {
			let rect = serde_json::json!({
				"low":  {"latitude": bbox.lat[0] + i as f64 * dlat,       "longitude": bbox.lon[0] + j as f64 * dlon},
				"high": {"latitude": bbox.lat[0] + (i + 1) as f64 * dlat, "longitude": bbox.lon[0] + (j + 1) as f64 * dlon},
			});
			for q in &cfg.queries {
				let mut token: Option<String> = None;
				// 3 pages x 20 is the API maximum for one text search
				for _ in 0..3 {
					let mut body = serde_json::json!({"textQuery": q, "pageSize": 20, "locationRestriction": {"rectangle": rect}});
					if let Some(t) = &token {
						body["pageToken"] = serde_json::Value::String(t.clone());
					}
					let res = work.cached_post(
						"https://places.googleapis.com/v1/places:searchText",
						&body,
						&[("X-Goog-Api-Key", &key), ("X-Goog-FieldMask", FIELDS)],
					)?;
					calls += 1;
					if let Some(e) = res.get("error") {
						bail!("Places text search {q:?}: {e}");
					}
					for p in res["places"].as_array().into_iter().flatten() {
						let id = p["id"].as_str().ok_or_else(|| eyre::eyre!("Places returned a result without an id: {p}"))?;
						raw.entry(id.to_owned()).or_insert_with(|| p.clone());
					}
					match res["nextPageToken"].as_str() {
						Some(t) => token = Some(t.to_owned()),
						None => break,
					}
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
	eprintln!("places: {} raw over {calls} calls, {} kept", raw.len(), out.len());
	Ok(out)
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
