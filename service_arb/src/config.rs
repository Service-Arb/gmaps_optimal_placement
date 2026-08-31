//! The study document. Not settings: there is no sane environment-variable spelling of `[[layer]]`,
//! so it is plain `Deserialize` from a path, with a JSON schema for the editor.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_arb_core::grid::Bbox;
use service_arb_sources::{GridSource, PoiConfig};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
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
	#[serde(rename = "layer")]
	pub layers: Vec<Layer>,
	/// Addresses the study is actually about. The usual question is "is *this* one good", not only
	/// "where is best".
	#[serde(default, rename = "candidate")]
	pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Area {
	pub bbox: Bbox,
	/// [lat, lon] the map opens at
	pub center: [f64; 2],
	pub zoom: u8,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridSpec {
	pub source: GridSource,
	pub vintage: u16,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Model {
	/// Per cell, over the grid's columns.
	pub demand: String,
	/// Per competitor, over the fields the POI source publishes.
	pub poi_weight: String,
	/// Catchment decay, metres. The map's opening slider position.
	pub lambda_m: f64,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Scale {
	/// Rank, not magnitude — these quantities are heavy-tailed and a linear ramp shows one hot pixel.
	#[default]
	Percentile,
	Linear,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Column {
	pub name: String,
	pub expr: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layer {
	pub name: String,
	pub expr: String,
	#[serde(default)]
	pub scale: Scale,
	#[serde(default)]
	pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
	pub name: String,
	/// [lat, lon]
	pub at: [f64; 2],
}
