#![feature(default_field_values)]
#![doc = include_str!("../README.md")]

pub mod config;
pub mod render;

use std::path::Path;

use eyre::{Result, WrapErr, bail, ensure};
use indexmap::IndexMap;
use regex::Regex;
use serde::Serialize;
pub use service_arb_core as core;
use service_arb_core::{Expr, LayerOut, PoiOut, TierOut};
pub use service_arb_core::{Payload, payload};
pub use service_arb_sources as sources;
use service_arb_sources::{Keyword, Work, grid, poi, searches};

use crate::config::Group;
pub use crate::config::Study;

/// One chart's worth of demand-side numbers. Members ride along with their own series, because a
/// sum you cannot audit is a sum you cannot argue with.
#[derive(Debug, Serialize)]
pub struct SearchPayload {
	pub name: String,
	pub place: String,
	/// `YYYY-MM`, shared by every group.
	pub months: Vec<String>,
	pub groups: Vec<GroupOut>,
}

#[derive(Debug, Serialize)]
pub struct GroupOut {
	pub name: String,
	/// Per month, summed over `members`.
	pub total: Vec<u64>,
	pub members: Vec<MemberOut>,
	/// Kept by the filter, but the provider has no volume for them. Never zero-filled into `total`.
	pub no_data: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MemberOut {
	pub text: String,
	pub avg: f64,
	pub monthly: Vec<u64>,
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

		for c in &self.columns {
			ensure!(!grid.columns.contains_key(&c.name), "derived column {:?} is already published by the source", c.name);
			let values = Expr::parse(&c.expr)?.eval_column(&grid.columns, n).wrap_err_with(|| format!("derived column {:?}", c.name))?;
			grid.columns.insert(c.name.clone(), values);
		}

		let demand = Expr::parse(&self.model.demand)?.eval_column(&grid.columns, n).wrap_err("[model] demand")?;
		let layers = self
			.layers
			.iter()
			.map(|l| {
				let values = Expr::parse(&l.expr)?.eval_column(&grid.columns, n).wrap_err_with(|| format!("layer {:?}", l.name))?;
				Ok(LayerOut {
					name: l.name.clone(),
					note: l.note.clone().unwrap_or_else(|| l.expr.clone()),
					scale: l.scale,
					values: round(values, 3),
				})
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
			tiers: self
				.poi
				.tiers
				.iter()
				.map(|t| TierOut {
					name: t.name.clone(),
					weight: t.weight,
				})
				.collect(),
			pois,
			candidates: self.candidates.clone(),
		})
	}

	pub fn searches(&self, work: &Work) -> Result<SearchPayload> {
		let cfg = self.searches.as_ref().ok_or_else(|| eyre::eyre!("study {:?} declares no `searches` block", self.name))?;
		ensure!(!cfg.groups.is_empty(), "searches declares no group");
		let provider = searches::provider(&cfg.provider)?;
		let (mut months, mut groups) = (Vec::new(), Vec::<GroupOut>::new());
		for g in &cfg.groups {
			ensure!(!groups.iter().any(|o| o.name == g.name), "two searches.group are both named {:?}", g.name);
			let ideas = provider.ideas(&g.seed, &cfg.place, &cfg.language, work)?;
			eprintln!("{}: {} ideas around {:?}", g.name, ideas.len(), g.seed);
			let (axis, out) = fold(g, ideas)?;
			match groups.first() {
				None => months = axis,
				Some(first) => ensure!(months == axis, "group {:?} covers {axis:?}, group {:?} covers {months:?}", g.name, first.name),
			}
			groups.push(out);
		}
		Ok(SearchPayload {
			name: self.name.clone(),
			place: cfg.place.clone(),
			months,
			groups,
		})
	}
}

/// The group filter and its sum. Kept out of the provider so a canned response can drive it.
pub fn fold(group: &Group, ideas: Vec<Keyword>) -> Result<(Vec<String>, GroupOut)> {
	let compile = |p: &String| Regex::new(&format!("(?i){p}")).wrap_err_with(|| format!("group {:?} pattern {p:?}", group.name));
	let keep = group.pattern.as_ref().map(&compile).transpose()?;
	let drop = group.drop.as_ref().map(&compile).transpose()?;

	let mut months: Vec<String> = Vec::new();
	let (mut members, mut no_data) = (Vec::new(), Vec::new());
	for k in ideas {
		if drop.as_ref().is_some_and(|r| r.is_match(&k.text)) || !keep.as_ref().is_none_or(|r| r.is_match(&k.text)) {
			continue;
		}
		let Some(series) = k.monthly else {
			no_data.push(k.text);
			continue;
		};
		ensure!(!series.is_empty(), "group {:?}: keyword {:?} carries an empty series", group.name, k.text);
		let axis: Vec<String> = series.iter().map(|m| format!("{:04}-{:02}", m.year, m.month)).collect();
		if months.is_empty() {
			months = axis;
		} else {
			ensure!(months == axis, "group {:?}: keyword {:?} covers {axis:?}, the group covers {months:?}", group.name, k.text);
		}
		let monthly: Vec<u64> = series.iter().map(|m| m.searches).collect();
		let avg = monthly.iter().sum::<u64>() as f64 / monthly.len() as f64;
		members.push(MemberOut {
			text: k.text,
			avg: round1(avg, 1),
			monthly,
		});
	}
	ensure!(
		!members.is_empty(),
		"group {:?} kept no keyword the provider has volume for ({} matched without data)",
		group.name,
		no_data.len()
	);

	let mut total = vec![0u64; months.len()];
	for m in &members {
		for (t, v) in total.iter_mut().zip(&m.monthly) {
			*t += v;
		}
	}
	members.sort_by(|a, b| b.avg.total_cmp(&a.avg));
	Ok((
		months,
		GroupOut {
			name: group.name.clone(),
			total,
			members,
			no_data,
		},
	))
}

pub fn load(path: &Path) -> Result<Study> {
	let out = std::process::Command::new("nix")
		.args(["eval", "--json", "--file"])
		.arg(path)
		.output()
		.wrap_err("running `nix eval` — a study is a Nix expression, so nix must be on PATH")?;
	ensure!(out.status.success(), "evaluating {}:\n{}", path.display(), String::from_utf8_lossy(&out.stderr).trim());
	let study: Study = serde_json::from_slice(&out.stdout).wrap_err_with(|| format!("parsing {}", path.display()))?;
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

pub fn search_stats(p: &SearchPayload) -> IndexMap<String, String> {
	let mut m = IndexMap::from([
		("place".to_owned(), p.place.clone()),
		("months".to_owned(), format!("{}..{}", p.months[0], p.months[p.months.len() - 1])),
	]);
	for g in &p.groups {
		let avg = g.total.iter().sum::<u64>() as f64 / g.total.len() as f64;
		m.insert(
			format!("{}_per_month", g.name),
			format!("{avg:.0} over {} keywords, {} without data", g.members.len(), g.no_data.len()),
		);
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
