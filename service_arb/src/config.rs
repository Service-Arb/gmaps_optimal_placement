//! The study document. Not settings: there is no sane environment-variable spelling of `[[layer]]`,
//! so it is plain `Deserialize` from a path, with a JSON schema for the editor.
use schemars::JsonSchema;
use serde::Deserialize;
use service_arb_core::grid::Bbox;
pub use service_arb_core::payload::{Candidate, Scale};
use service_arb_sources::{GridSource, PoiConfig};

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Study {
	pub name: String,
	pub area: Area,
	pub grid: GridSpec,
	pub poi: PoiConfig,
	/// Columns the source does not publish, evaluated in order over the ones it does. Later entries
	/// may read earlier ones, so this is a list: an attribute set would be evaluated in whatever
	/// order its names happen to sort in.
	#[serde(default, rename = "column")]
	pub columns: Vec<Column>,
	pub model: Model,
	pub rank: RankSpec,
	#[serde(rename = "layer")]
	pub layers: Vec<Layer>,
	/// Optional: the map answers where the people are, this answers how many of them ask for the
	/// thing. A study without it simply has no `searches` command.
	#[serde(default)]
	pub searches: Option<Searches>,
	/// Addresses the study is actually about. The usual question is "is *this* one good", not only
	/// "where is best".
	#[serde(default, rename = "candidate")]
	pub candidates: Vec<Candidate>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Area {
	pub bbox: Bbox,
	/// [lat, lon] the map opens at
	pub center: [f64; 2],
	pub zoom: u8,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridSpec {
	pub source: GridSource,
	pub vintage: u16,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Model {
	/// Per cell, over the grid's columns.
	pub demand: String,
	/// Catchment decay, metres. The map's opening slider position.
	pub lambda_m: f64,
}

/// What the study asks Google, and how much each answer counts. The reviews→prominence curve is
/// not here: it is a property of how Google ranks, shared by every trade, and lives in
/// `service_arb_core::rank`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RankSpec {
	/// Power of two — a k-d split to depth log2(n), one equal share of demand per stratum.
	pub nodes: usize,
	/// The `locationBias` circle, metres.
	pub radius_m: f64,
	/// Deliberately not `poi.queries`: those are tuned to *find* competitors, which biases them
	/// toward words already in the shop names. The name coefficient is identified by businesses that
	/// were close and did not appear, so this set must contain terms the names do not.
	#[serde(rename = "term")]
	pub terms: Vec<Term>,
}

/// One query, and what a searcher typing it is worth — this is where searches × ticket price enters.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Term {
	pub text: String,
	pub weight: f64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Column {
	pub name: String,
	pub expr: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layer {
	pub name: String,
	pub expr: String,
	#[serde(default)]
	pub scale: Scale,
	#[serde(default)]
	pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Searches {
	/// `google_ads` or `dataforseo`.
	pub provider: String,
	/// A Google canonical location name, e.g. `Clermont-Ferrand,Auvergne-Rhone-Alpes,France`.
	/// DataForSEO's `location_name` format is the same string.
	pub place: String,
	/// ISO-639-1.
	pub language: String,
	#[serde(rename = "group")]
	pub groups: Vec<Group>,
}

/// One line on the chart: an intent, asked for in however many words people ask for it in.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Group {
	pub name: String,
	/// What the provider expands semantically around.
	pub seed: Vec<String>,
	/// Case-insensitive regex over the expansion. Omitted keeps all of it.
	#[serde(default, rename = "match")]
	pub pattern: Option<String>,
	/// Checked before `match` — expansion drags in intent that is not demand.
	#[serde(default)]
	pub drop: Option<String>,
}
