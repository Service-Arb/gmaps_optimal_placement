#![doc = include_str!("../README.md")]

pub mod config;
pub mod render;

pub use service_arb_core as core;
pub use service_arb_sources as sources;

use eyre::{Result, WrapErr, bail, ensure};
use indexmap::IndexMap;
use serde::Serialize;
use service_arb_core::Expr;
use service_arb_sources::{Work, grid, poi};

pub use crate::config::Study;
use crate::config::{Candidate, Scale};

/// Everything the map needs, already evaluated. What stays in JS is only what the sliders move.
#[derive(Debug, Serialize)]
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
	pub pois: Vec<PoiOut>,
	pub candidates: Vec<Candidate>,
}

#[derive(Debug, Serialize)]
pub struct LayerOut {
	pub name: String,
	pub note: String,
	pub scale: Scale,
	pub values: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub struct TierOut {
	pub name: String,
	pub weight: f64,
}

#[derive(Debug, Serialize)]
pub struct PoiOut {
	#[serde(flatten)]
	pub poi: poi::Poi,
	pub w: f64,
}

/// The grid and everything derived from it. Everything up to, and not including, the billed part.
pub struct Cells {
	pub grid: service_arb_core::Grid,
	pub demand: Vec<f64>,
	pub layers: Vec<LayerOut>,
}

impl Study {
	/// Fails on the first expression that does not hold against the source's columns, before any
	/// POI call is made.
	pub fn cells(&self, work: &Work) -> Result<Cells> {
		let mut grid = grid::load(self.grid.source, self.grid.vintage, self.area.bbox, work)?;
		let n = grid.len();

		for (name, src) in &self.columns {
			ensure!(!grid.columns.contains_key(name), "derived column {name:?} is already published by the source");
			let values = Expr::parse(src)?.eval_column(&grid.columns, n).wrap_err_with(|| format!("derived column {name:?}"))?;
			grid.columns.insert(name.clone(), values);
		}

		let demand = Expr::parse(&self.model.demand)?.eval_column(&grid.columns, n).wrap_err("[model] demand")?;
		let layers = self
			.layers
			.iter()
			.map(|l| {
				let values = Expr::parse(&l.expr)?.eval_column(&grid.columns, n).wrap_err_with(|| format!("layer {:?}", l.name))?;
				Ok(LayerOut { name: l.name.clone(), note: l.note.clone().unwrap_or_else(|| l.expr.clone()), scale: l.scale, values: round(values, 3) })
			})
			.collect::<Result<Vec<_>>>()?;
		Ok(Cells { grid, demand, layers })
	}

	pub fn build(&self, work: &Work) -> Result<Payload> {
		let Cells { grid: g, demand, layers } = self.cells(work)?;
		let n = g.len();
		let weight = Expr::parse(&self.model.poi_weight)?;
		let pois = poi::load(&self.poi, self.area.bbox, work)?
			.into_iter()
			.map(|p| {
				let w = weight.eval_row(&p.fields()).wrap_err_with(|| format!("[model] poi_weight at {:?}", p.name))?;
				Ok(PoiOut { poi: p, w })
			})
			.collect::<Result<Vec<_>>>()?;
		for t in &self.poi.tiers {
			ensure!(pois.iter().any(|p| p.poi.tier == t.name), "no competitor fell into tier {:?}", t.name);
		}

		let mut ring = Vec::with_capacity(n * 8);
		for c in &g.cells {
			for p in c.ring {
				// 5 decimals is ~1 m: below the cell size, and a third of the file size of full f64
				ring.push(round1(p[0], 5));
				ring.push(round1(p[1], 5));
			}
		}
		Ok(Payload {
			name: self.name.clone(),
			center: self.area.center,
			zoom: self.area.zoom,
			lambda_m: self.model.lambda_m,
			demand_note: self.model.demand.clone(),
			ring,
			place: g.cells.iter().map(|c| c.place.clone()).collect(),
			imputed: g.cells.iter().map(|c| u8::from(c.imputed)).collect(),
			demand: round(demand, 3),
			layers,
			tiers: self.poi.tiers.iter().map(|t| TierOut { name: t.name.clone(), weight: t.weight }).collect(),
			pois,
			candidates: self.candidates.clone(),
		})
	}
}

pub fn load(path: &std::path::Path) -> Result<Study> {
	let text = std::fs::read_to_string(path).wrap_err_with(|| format!("reading {}", path.display()))?;
	let study: Study = toml::from_str(&text).wrap_err_with(|| format!("parsing {}", path.display()))?;
	if study.layers.is_empty() {
		bail!("{} declares no [[layer]]", path.display());
	}
	Ok(study)
}

/// Summary statistics, so a model change that moves numbers is visible rather than silent.
pub fn stats(p: &Payload) -> IndexMap<String, String> {
	let sum = |v: &[f64]| v.iter().sum::<f64>();
	let mut m = IndexMap::from([
		("cells".to_owned(), p.place.len().to_string()),
		("imputed_cells".to_owned(), p.imputed.iter().filter(|&&i| i == 1).count().to_string()),
		("demand_total".to_owned(), format!("{:.1}", sum(&p.demand))),
		("competitors".to_owned(), p.pois.len().to_string()),
	]);
	for t in &p.tiers {
		m.insert(format!("tier_{}", t.name), p.pois.iter().filter(|q| q.poi.tier == t.name).count().to_string());
	}
	for l in &p.layers {
		m.insert(format!("layer_{}_total", l.name.to_lowercase().replace(' ', "_")), format!("{:.1}", sum(&l.values)));
	}
	m
}

fn round(v: Vec<f64>, places: i32) -> Vec<f64> {
	v.into_iter().map(|x| round1(x, places)).collect()
}

fn round1(x: f64, places: i32) -> f64 {
	let s = 10f64.powi(places);
	(x * s).round() / s
}
