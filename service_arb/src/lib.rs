#![feature(default_field_values)]
#![doc = include_str!("../README.md")]

pub mod config;
pub mod fit;
pub mod render;

use std::path::Path;

use eyre::{Result, WrapErr, bail, ensure};
use indexmap::IndexMap;
use regex::Regex;
use serde::Serialize;
pub use service_arb_core as core;
use service_arb_core::{
	Expr, LayerOut, PoiOut, TierOut,
	rank::{self, Biz, Feats, Rank},
};
pub use service_arb_core::{Payload, payload};
pub use service_arb_sources as sources;
use service_arb_sources::{Keyword, Ranking, Work, grid, poi, probe, searches};

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

	/// The ordering evidence this study contributes: the inventory sweep always, the probe wherever
	/// it has already been run. Reads the work dir, never the network.
	pub fn observations(&self, work: &Work) -> Result<(Vec<core::rank::Obs>, (f64, f64))> {
		let cells = self.cells(work)?;
		let inv = poi::load(&self.poi, self.area.bbox, work)?;
		let mut obs = inv.obs;
		let probed = probe::cached(work, &probe::plan(&at(&self.nodes(&cells)?), &self.terms(), self.rank.radius_m))?;
		eprintln!("{}: {} orderings from the inventory sweep, {} from the probe", self.name, obs.len(), probed.len());
		obs.extend(probed);
		let places: Vec<String> = cells.grid.cells.iter().map(|c| c.place.clone()).collect();
		fit::observations(&feats(&places, &inv.pois)?, &inv.pois, &obs)
	}

	pub fn probe(&self, work: &Work, dry: bool) -> Result<Vec<Ranking>> {
		let nodes = self.nodes(&self.cells(work)?)?;
		let plan = probe::plan(&at(&nodes), &self.terms(), self.rank.radius_m);
		if dry {
			for n in &nodes {
				eprintln!("  {:.5}, {:.5}   {:.1}% of demand", n.at[0], n.at[1], 100. * n.share);
			}
			eprintln!("{} searches, one call each: {} Text Search Essentials", plan.len(), plan.len());
			return Ok(Vec::new());
		}
		let out = probe::run(work, plan)?;
		eprintln!("probe: {} orderings over {} billed calls", out.len(), work.billed());
		Ok(out)
	}

	fn terms(&self) -> Vec<String> {
		self.rank.terms.iter().map(|t| t.text.clone()).collect()
	}

	fn nodes(&self, cells: &Cells) -> Result<Vec<rank::Node>> {
		let at: Vec<[f64; 2]> = cells
			.grid
			.cells
			.iter()
			.map(|c| {
				let (lon, lat) = c.ring.iter().fold((0., 0.), |(x, y), p| (x + p[0] / 4., y + p[1] / 4.));
				[lat, lon]
			})
			.collect();
		rank::nodes(&at, &cells.demand, self.rank.nodes)
	}

	pub fn build(&self, work: &Work) -> Result<Payload> {
		let Cells { grid: g, demand, layers } = self.cells(work)?;
		let n = g.len();
		let place: Vec<String> = g.cells.iter().map(|c| c.place.clone()).collect();
		let inv = poi::load(&self.poi, self.area.bbox, work)?;
		let model = Rank::try_new(feats(&place, &inv.pois)?, rank::COEF)?;
		let terms: Vec<(String, f64)> = self.rank.terms.iter().map(|t| (t.text.clone(), t.weight)).collect();
		let mut pois: Vec<PoiOut> = inv
			.pois
			.into_iter()
			.map(|p| {
				let w = model.strength(&Biz::from(&p), &terms);
				PoiOut { poi: p, w }
			})
			.collect();
		normalise(&mut pois, &self.poi.tiers)?;
		for t in &self.poi.tiers {
			ensure!(pois.iter().any(|p| p.poi.tier == t.name), "no competitor fell into tier {:?}", t.name);
		}
		for (i, c) in self.candidates.iter().enumerate() {
			// the pin file hides by name, so a duplicate would hide two pins at once
			ensure!(!self.candidates[..i].iter().any(|o| o.name == c.name), "two [[candidate]] are both named {:?}", c.name);
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
			place,
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
			terms: terms.into_iter().map(|(text, weight)| payload::TermOut { text, weight }).collect(),
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
	let mut w: Vec<f64> = p.pois.iter().map(|q| q.w).collect();
	w.sort_by(f64::total_cmp);
	let mut m = IndexMap::from([
		("cells".to_owned(), p.place.len().to_string()),
		("imputed_cells".to_owned(), p.imputed.iter().filter(|&&i| i == 1).count().to_string()),
		("demand_total".to_owned(), format!("{:.1}", sum(&p.demand))),
		("competitors".to_owned(), p.pois.len().to_string()),
		(
			"competitor_w".to_owned(),
			format!("total {:.2}, median {:.2}, max {:.2}", sum(&w), w[w.len() / 2], w[w.len() - 1]),
		),
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
fn at(nodes: &[rank::Node]) -> Vec<[f64; 2]> {
	nodes.iter().map(|n| n.at).collect()
}

fn feats(places: &[String], pois: &[payload::Poi]) -> Result<Feats> {
	let rated: Vec<f64> = pois.iter().filter_map(|p| p.rating).collect();
	ensure!(!rated.is_empty(), "no competitor carries a rating, so there is nothing to shrink towards");
	Feats::try_new(rated.iter().sum::<f64>() / rated.len() as f64, places)
}

/// `w = 1` is "a competitor as strong as the thing I am about to open" — `Model::capture_at` gives a
/// candidate's own pull an implicit weight of 1.0. Per tier, because `Model::pressure` already
/// multiplies by the tier weight: a fitted strength that has learned a wash ranks below a detailer
/// would otherwise be discounted twice, and the tier slider would stop being the only cross-tier
/// statement in the study.
fn normalise(pois: &mut [PoiOut], tiers: &[service_arb_sources::poi::Tier]) -> Result<()> {
	for t in tiers {
		let mut w: Vec<f64> = pois.iter().filter(|p| p.poi.tier == t.name).map(|p| p.w).collect();
		if w.is_empty() {
			continue;
		}
		w.sort_by(f64::total_cmp);
		let med = w[w.len() / 2];
		ensure!(med > 0., "tier {:?} has a median strength of {med}, so it cannot be normalised", t.name);
		for p in pois.iter_mut().filter(|p| p.poi.tier == t.name) {
			p.w = round1(p.w / med, 4);
		}
	}
	Ok(())
}

fn round(v: Vec<f64>, places: i32) -> Vec<f64> {
	v.into_iter().map(|x| round1(x, places)).collect()
}

fn round1(x: f64, places: i32) -> f64 {
	let s = 10f64.powi(places);
	(x * s).round() / s
}
