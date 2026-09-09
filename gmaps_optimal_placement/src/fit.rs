//! Estimating the ranking model from orderings already on disk.
//!
//! Adam over `gmaps_optimal_placement_core::rank`'s likelihood. Offline and slow-ish by design: it runs once,
//! its output is a table of five numbers, and those numbers are pasted into `rank::COEF` and
//! committed.
use eyre::{Result, ensure};
use gmaps_optimal_placement_core::{
	payload::Poi,
	rank::{self, Biz, DIST, Feats, K, N, NAMES, Obs},
};
use gmaps_optimal_placement_sources::{Ranking, Region};
use indexmap::IndexMap;

const STEPS: usize = 2000;
const LR: f64 = 0.02;
/// A business absent from a search is absent for four reasons and only one of them is "weak": past
/// the page cap (informative), outside the search area (a filter, not a ranking), irrelevant to the
/// term, or deduped. So the choice set is what the *same term* returned anywhere — asking why a
/// plumber did not appear in car-wash results collapses the fit to all-zero — intersected with the
/// area this search could reach. For a node that is `REACH` times the furthest result it did return,
/// which is self-calibrating and needs no magic radius; for a tile it is the tile.
const REACH: f64 = 1.5;

pub struct Fit {
	pub coef: [f64; N],
	pub nll: f64,
	obs: Vec<Obs>,
	/// node→business over everything Places actually returned, km.
	reach: (f64, f64),
}
impl Fit {
	/// Each coefficient, and what the likelihood loses when that feature is refitted out.
	pub fn stats(&self, lambda_m: &[f64]) -> IndexMap<String, String> {
		let mut m = IndexMap::from([
			("orderings".to_owned(), self.obs.len().to_string()),
			(
				"choice_set".to_owned(),
				format!(
					"{}..{} places, top {K} scored",
					self.obs.iter().map(|o| o.x.len()).min().unwrap_or(0),
					self.obs.iter().map(|o| o.x.len()).max().unwrap_or(0)
				),
			),
			("nll".to_owned(), format!("{:.1}", self.nll)),
		]);
		for i in 0..N {
			let mut on = [true; N];
			on[i] = false;
			let (_, without) = adam(&self.obs, on);
			m.insert(NAMES[i].to_owned(), format!("{:+.4}   Δnll if dropped {:+.1}", self.coef[i], without - self.nll));
		}
		m.insert(
			"implied_lambda_m".to_owned(),
			format!(
				"{:.0} (studies say {})",
				-1000. / self.coef[DIST],
				lambda_m.iter().map(|l| format!("{l:.0}")).collect::<Vec<_>>().join(", ")
			),
		);
		m.insert(
			"reach_km".to_owned(),
			match self.reach.0 <= self.reach.1 {
				true => format!("{:.2}..{:.2}", self.reach.0, self.reach.1),
				false => "none — no ordering was asked from a node".to_owned(),
			},
		);
		m.insert("top3_overlap".to_owned(), format!("{:.2}", self.top3()));
		m
	}

	/// A catchment the data never observed is extrapolation, and a coefficient that says distance
	/// helps is a data problem. Both are cheap to check and neither is recoverable.
	pub fn check(&self) -> Result<()> {
		ensure!(self.coef[DIST] < 0., "distance coefficient is {:+.4}: the fit says further away ranks higher", self.coef[DIST]);
		let lambda = -1. / self.coef[DIST];
		ensure!(
			(self.reach.0..=self.reach.1).contains(&lambda),
			"implied catchment {lambda:.2} km falls outside the {:.2}..{:.2} km the probe observed — nothing identifies it there",
			self.reach.0,
			self.reach.1
		);
		Ok(())
	}

	/// How often the model puts the same three businesses on top as Google did. The quantity that
	/// matters, unlike the likelihood, which is only what the optimiser could differentiate.
	fn top3(&self) -> f64 {
		let (mut sum, mut n) = (0., 0);
		for o in self.obs.iter().filter(|o| o.ranked >= 3) {
			let mut by_score: Vec<usize> = (0..o.x.len()).collect();
			let s = |j: usize| (0..N).map(|i| self.coef[i] * o.x[j][i]).sum::<f64>();
			by_score.sort_by(|&a, &b| s(b).total_cmp(&s(a)));
			sum += by_score[..3].iter().filter(|&&j| j < 3).count() as f64 / 3.;
			n += 1;
		}
		sum / n as f64
	}
}

/// How Google ranks is one mechanism, so the evidence pools: every study's orderings go into one
/// coefficient set, and each study's own term weights re-weight it afterwards.
impl FromIterator<(Vec<Obs>, (f64, f64))> for Fit {
	fn from_iter<I: IntoIterator<Item = (Vec<Obs>, (f64, f64))>>(it: I) -> Self {
		let (mut obs, mut reach) = (Vec::new(), (f64::MAX, 0f64));
		for (o, r) in it {
			obs.extend(o);
			reach = (reach.0.min(r.0), reach.1.max(r.1));
		}
		let (coef, nll) = adam(&obs, [true; N]);
		Self { coef, nll, obs, reach }
	}
}

/// One choice set per search, with the returned ids first and in order.
pub fn observations(feats: &Feats, pois: &[Poi], rankings: &[Ranking]) -> Result<(Vec<Obs>, (f64, f64))> {
	let by_id: IndexMap<&str, &Poi> = pois.iter().map(|p| (p.id.as_str(), p)).collect();
	// attributes join by id, so a result the inventory dropped cannot be scored and leaves the set
	let known = |ids: &[String]| -> Vec<&str> {
		let mut out: Vec<&str> = Vec::new();
		for id in ids {
			if let Some((id, _)) = by_id.get_key_value(id.as_str())
				&& !out.contains(id)
			{
				out.push(id);
			}
		}
		out
	};

	let mut pool: IndexMap<&str, Vec<&str>> = IndexMap::new();
	for r in rankings {
		let seen = pool.entry(r.term.as_str()).or_default();
		for id in known(&r.ids) {
			if !seen.contains(&id) {
				seen.push(id);
			}
		}
	}

	let (mut obs, mut reach) = (Vec::new(), (f64::MAX, 0f64));
	for r in rankings {
		let seen = known(&r.ids);
		if seen.len() < 2 {
			continue;
		}
		let node = match r.from {
			Region::Tile(_) => None,
			Region::Node(at, _) => Some(at),
		};
		let mut x: Vec<[f64; N]> = seen.iter().map(|id| feats.at(&Biz::from(by_id[id]), &r.term, node)).collect();
		let far = x.iter().map(|x| x[DIST]).fold(0., f64::max);
		if node.is_some() {
			reach = (reach.0.min(x.iter().map(|x| x[DIST]).fold(f64::MAX, f64::min)), reach.1.max(far));
		}

		for id in pool[r.term.as_str()].iter().filter(|id| !seen.contains(id)) {
			let p = by_id[id];
			let reachable = match r.from {
				Region::Tile(b) => (b.lat[0]..=b.lat[1]).contains(&p.lat) && (b.lon[0]..=b.lon[1]).contains(&p.lng),
				Region::Node(..) => feats.at(&Biz::from(p), &r.term, node)[DIST] <= REACH * far,
			};
			if reachable {
				x.push(feats.at(&Biz::from(p), &r.term, node));
			}
		}
		obs.push(Obs { x, ranked: seen.len() });
	}
	ensure!(!obs.is_empty(), "no ordering carried two businesses the inventory knows");
	Ok((obs, reach))
}

/// Full-batch Adam. Five parameters over a few hundred orderings — batching would buy noise, and
/// the whole run is seconds.
fn adam(obs: &[Obs], on: [bool; N]) -> ([f64; N], f64) {
	let (mut c, mut m, mut v) = ([0.; N], [0.; N], [0.; N]);
	for t in 1..=STEPS {
		let (_, g) = rank::nll_grad(obs, &c);
		for i in (0..N).filter(|&i| on[i]) {
			m[i] = 0.9 * m[i] + 0.1 * g[i];
			v[i] = 0.999 * v[i] + 0.001 * g[i] * g[i];
			let (mh, vh) = (m[i] / (1. - 0.9f64.powi(t as i32)), v[i] / (1. - 0.999f64.powi(t as i32)));
			c[i] -= LR * mh / (vh.sqrt() + 1e-8);
		}
	}
	let (nll, _) = rank::nll_grad(obs, &c);
	(c, nll)
}
