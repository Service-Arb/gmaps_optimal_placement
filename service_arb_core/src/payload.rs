//! What the browser is handed: the study, already evaluated. Everything a slider cannot move.
//!
//! The shape lives here rather than beside the CLI because the map reads it back, and the map is
//! wasm — it may not link a single line of I/O.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Payload {
	pub name: String,
	pub center: [f64; 2],
	pub zoom: u8,
	pub lambda_m: f64,
	/// The demand expression itself — the map has no other honest caption for it.
	pub demand_note: String,
	/// 8 numbers per cell: (lon, lat) SW, SE, NE, NW.
	pub ring: Vec<f64>,
	pub place: Vec<String>,
	pub imputed: Vec<u8>,
	pub demand: Vec<f64>,
	pub layers: Vec<LayerOut>,
	pub tiers: Vec<TierOut>,
	/// The study's queries and what each is worth. `w` is already scored against them; the what-if
	/// needs them because the business it scores does not exist yet.
	pub terms: Vec<TermOut>,
	pub pois: Vec<PoiOut>,
	pub candidates: Vec<Candidate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TermOut {
	pub text: String,
	pub weight: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LayerOut {
	pub name: String,
	pub note: String,
	pub scale: Scale,
	pub values: Vec<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TierOut {
	pub name: String,
	pub weight: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PoiOut {
	#[serde(flatten)]
	pub poi: Poi,
	/// How much this one counts, from `rank`, normalised so the median of its tier is 1.
	pub w: f64,
}

/// Whoever is already selling the service. The inventory that produces one is `service_arb_sources`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Poi {
	pub id: String,
	pub name: String,
	pub addr: String,
	pub lat: f64,
	pub lng: f64,
	pub rating: Option<f64>,
	pub n_rev: f64,
	pub kind: String,
	pub kind_label: String,
	pub web: String,
	pub tel: String,
	pub tier: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scale {
	/// Rank, not magnitude — these quantities are heavy-tailed and a linear ramp shows one hot pixel.
	#[default]
	Percentile,
	Linear,
}

/// An address the study is actually about. The usual question is "is *this* one good", not only
/// "where is best".
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
	pub name: String,
	/// [lat, lon]
	pub at: [f64; 2],
}
