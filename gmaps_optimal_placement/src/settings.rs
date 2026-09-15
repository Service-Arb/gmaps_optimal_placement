//! What the tool does, rather than what a study is about. A study document is the two axes of the
//! product and nothing else; anything that would be the same for every study — the quota it spends
//! against, how old an answer may get before the map says so — lives here.
use gmaps_optimal_placement_sources::work::Age as Ages;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use v_utils::{
	Timeframe,
	macros::{ConfigJsonSchema, MyConfigPrimitives, Settings, SettingsNested},
};

#[derive(Clone, ConfigJsonSchema, Debug, MyConfigPrimitives, Settings)]
pub struct AppConfig {
	#[settings(flatten)]
	pub places: Places,
	#[settings(flatten)]
	pub age: Age,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, SettingsNested, SmartDefault)]
#[serde(default)]
pub struct Places {
	/// `SearchTextRequestPerDayPerProject`. An API key cannot read a project's quota — both
	/// `serviceusage` and `monitoring` want an OAuth2 principal — so it is stated here, counted
	/// locally, and corrected from any 429 naming a different one.
	#[default(100)]
	pub per_day: u32,
}

/// How old an answer may get before the map says so. Nothing is evicted and nothing is refetched:
/// a refetch spends a day of the quota above to learn what is mostly the same thing, so age is
/// reported and `--refresh` is the only thing that re-asks.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, SettingsNested, SmartDefault)]
#[serde(default)]
pub struct Age {
	/// The inventory sweep: shop names, ratings, addresses. Shops open and close.
	#[default(Ages::INVENTORY.into())]
	pub inventory: Timeframe,
	/// Probe orderings — who ranks where from each stratum. What `core::rank::COEF` is fitted on.
	#[default(Ages::ORDERING.into())]
	pub ordering: Timeframe,
	/// The commune list from geo.api.gouv.fr. Drifts only on mergers.
	#[default(Ages::COMMUNES.into())]
	pub communes: Timeframe,
}

impl From<&Age> for Ages {
	fn from(a: &Age) -> Self {
		Self {
			inventory: a.inventory.duration(),
			ordering: a.ordering.duration(),
			communes: a.communes.duration(),
		}
	}
}
