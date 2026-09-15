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
/// Three pages of twenty is all one text search will hand back, so a tile that comes back at this
/// has more in it than it said.
const CAP: usize = 60;
/// How far a saturated tile may be quartered. Over a city frame the floor is a couple of
/// kilometres, and a query still at the cap down there is reported rather than passed off as the
/// whole inventory.
const DEPTH: u32 = 3;
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
///
/// No key is the read-only mode: pages come out of the work dir and the first miss ends the search.
/// A caller holding no key cannot buy, which is what lets the fit replay a sweep rather than pay
/// for one.
pub fn search_text(work: &Work, key: Option<&str>, body: &serde_json::Value, mask: &str, pages: usize) -> Result<Vec<(String, serde_json::Value)>> {
	let mut out = Vec::new();
	let mut token: Option<String> = None;
	for _ in 0..pages {
		let mut body = body.clone();
		if let Some(t) = &token {
			body["pageToken"] = serde_json::Value::String(t.clone());
		}
		let res = match key {
			Some(k) => work.cached_post(SEARCH_TEXT, &body, &[("X-Goog-Api-Key", k), ("X-Goog-FieldMask", mask)])?,
			None => match work.cached(SEARCH_TEXT, &body)? {
				Some(hit) => hit,
				None => break,
			},
		};
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
	let key = std::env::var("GOOGLE_MAPS_KEY").wrap_err("GOOGLE_MAPS_KEY is not set")?;
	sweep(cfg, bbox, Some(&key), work)
}

/// The same sweep, restricted to what the work dir already holds. A pairing nobody ever harvested
/// yields nothing, which is how a refit reads evidence without turning into a purchase.
pub fn cached(cfg: &PoiConfig, bbox: Bbox, work: &Work) -> Result<Inventory> {
	sweep(cfg, bbox, None, work)
}

fn sweep(cfg: &PoiConfig, bbox: Bbox, key: Option<&str>, work: &Work) -> Result<Inventory> {
	let PoiSource::GooglePlaces = cfg.source;
	let drop = cfg.drop.as_ref().map(MatchSpec::compile).transpose()?;
	let tiers: Vec<(&Tier, Matcher)> = cfg.tiers.iter().map(|t| Ok((t, t.spec.compile()?))).collect::<Result<_>>()?;
	ensure!(!tiers.is_empty(), "at least one [[poi.tier]] is needed to say what a competitor is");

	let mut found = Found::default();
	for q in &cfg.queries {
		// every answer that arrived is already on disk and the recursion is deterministic, so whatever
		// stopped this — a daily quota above all — a rerun picks up where it stopped and pays only for
		// what is still missing
		descend(work, key, q, bbox, 0, &mut found).wrap_err_with(|| format!("sweeping {q:?}: a rerun resumes from here, and re-asks nothing already answered"))?;
		eprintln!("  {q:?}: {} orderings, {} billed so far", found.obs.len(), work.billed());
	}
	let Found { raw, obs, censored } = found;
	if !censored.is_empty() {
		eprintln!(
			"places: {} tiles are still at the {CAP}-result cap at depth {DEPTH}, so the inventory under them is partial:",
			censored.len()
		);
		for (q, t) in &censored {
			eprintln!("  {q:?} over lat {:?} lon {:?}", t.lat, t.lon);
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
	eprintln!("places: {} raw over {} searches ({} billed), {} kept", raw.len(), obs.len(), work.billed(), out.len());
	Ok(Inventory { pois: out, obs })
}

/// One query over one tile, quartered wherever the answer came back at the cap. Splitting is per
/// query and not once for the whole sweep: "lavage auto" saturates downtown where "covering
/// carrosserie" does not, and the quadrants of a tile nobody filled are not worth asking.
///
/// Not a study's to state: the cap bites on competitor density, which is a fact about the trade and
/// the ground together, and which no document knows before it asks.
fn descend(work: &Work, key: Option<&str>, query: &str, tile: Bbox, depth: u32, found: &mut Found) -> Result<()> {
	// the cache key is the body verbatim, so a tile's corners may not be re-associated
	let rect = serde_json::json!({
		"low":  {"latitude": tile.lat[0], "longitude": tile.lon[0]},
		"high": {"latitude": tile.lat[1], "longitude": tile.lon[1]},
	});
	let body = serde_json::json!({"textQuery": query, "pageSize": 20, "locationRestriction": {"rectangle": rect}});
	let page = search_text(work, key, &body, FIELDS, 3)?;
	// an ordering of nothing identifies nothing, and under a read-only key it is also what a tile
	// that was never harvested looks like
	if page.is_empty() {
		return Ok(());
	}
	found.obs.push(Ranking {
		from: Region::Tile(tile),
		term: query.to_owned(),
		ids: page.iter().map(|(id, _)| id.clone()).collect(),
	});
	for (id, p) in page.iter() {
		found.raw.entry(id.clone()).or_insert_with(|| p.clone());
	}
	if page.len() < CAP {
		return Ok(());
	}
	if depth == DEPTH {
		found.censored.push((query.to_owned(), tile));
		return Ok(());
	}
	let (mid_lat, mid_lon) = ((tile.lat[0] + tile.lat[1]) / 2., (tile.lon[0] + tile.lon[1]) / 2.);
	for lat in [[tile.lat[0], mid_lat], [mid_lat, tile.lat[1]]] {
		for lon in [[tile.lon[0], mid_lon], [mid_lon, tile.lon[1]]] {
			descend(work, key, query, Bbox { lat, lon }, depth + 1, found)?;
		}
	}
	Ok(())
}

/// What the recursion accumulates, before the tiering rules turn it into an `Inventory`.
#[derive(Default)]
struct Found {
	raw: IndexMap<String, serde_json::Value>,
	obs: Vec<Ranking>,
	/// Query and tile still at the cap at the depth floor: the inventory under them is partial.
	censored: Vec<(String, Bbox)>,
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
