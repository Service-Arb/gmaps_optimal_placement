//! Everything the sliders move: competitor pressure, underserved demand, the capture score and the
//! top-N sweep, plus the colouring and the report rows they feed.
//!
//! No I/O and no `google.maps` — the browser recomputes here, and `cargo t` checks the same code
//! against a fixture.
use eyre::{Result, ensure};
use serde::Serialize;

use crate::payload::{Candidate, Payload, PoiOut, Scale};

const R: f64 = 6378137.;
/// Beyond 3λ the exponential contributes under 5 % of one competitor — not worth the distance.
const CUTOFF: f64 = 3.;

/// Turbo, sampled at eight stops. Quantile colouring: these values are heavy-tailed, a linear ramp
/// would show one hot pixel downtown and nothing else.
const TURBO: [[f64; 3]; 8] = [
	[48., 18., 59.],
	[70., 107., 227.],
	[38., 187., 203.],
	[122., 231., 111.],
	[219., 213., 50.],
	[249., 132., 42.],
	[199., 42., 17.],
	[122., 4., 3.],
];
/// Web-Mercator world coords, 256 px at zoom 0. Precomputed so a redraw is arithmetic instead of
/// 40k projection calls per frame.
pub fn world_x(lng: f64) -> f64 {
	(lng + 180.) / 360. * 256.
}

pub fn world_y(lat: f64) -> f64 {
	let s = (lat.to_radians()).sin();
	128. - 256. / (4. * std::f64::consts::PI) * ((1. + s) / (1. - s)).ln()
}

/// Live tier state — the checkbox and the weight slider.
#[derive(Clone, Debug)]
pub struct TierState {
	pub name: String,
	pub weight: f64,
	pub show: bool,
}
/// Which array a layer paints. The first three are functions of the sliders; the rest were
/// evaluated at build time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerRef {
	Unmet,
	Pressure,
	Demand,
	Study(usize),
}
pub struct LayerSpec {
	pub name: String,
	pub note: String,
	/// Magnitude rather than rank.
	pub linear: bool,
	pub source: LayerRef,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Row {
	pub label: String,
	/// Second line under the label: whatever the row measured against.
	pub sub: Option<String>,
	/// Empty makes the row a section heading.
	pub value: String,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Report {
	pub title: String,
	pub rows: Vec<Row>,
	pub note: String,
}
/// A cell the top-N sweep picked.
#[derive(Debug, Serialize)]
pub struct Ranked {
	pub cell: usize,
	pub lat: f64,
	pub lng: f64,
	pub score: f64,
}
/// The study's geometry, projected once. Everything below reads it and the live control values.
pub struct Model {
	pub payload: Payload,
	/// `n * 4` Web-Mercator corners, in the canvas loop's own order.
	pub ring_x: Vec<f64>,
	pub ring_y: Vec<f64>,
	pub c_lat: Vec<f64>,
	pub c_lng: Vec<f64>,
	/// Cell centroids in metres from the grid's own centre. Flat-earth over one agglomeration.
	mx: Vec<f64>,
	my: Vec<f64>,
	lat0: f64,
	lng0: f64,
	m_per_lng: f64,
	m_per_lat: f64,
	comps: Vec<Comp>,
}
impl Model {
	pub fn try_new(payload: Payload) -> Result<Self> {
		let n = payload.place.len();
		ensure!(n > 0, "payload carries no cells");
		ensure!(payload.ring.len() == n * 8, "payload carries {} ring numbers for {n} cells", payload.ring.len());
		ensure!(payload.demand.len() == n && payload.imputed.len() == n, "payload arrays disagree on the cell count");
		for l in &payload.layers {
			ensure!(l.values.len() == n, "layer {:?} carries {} values for {n} cells", l.name, l.values.len());
		}

		let (mut ring_x, mut ring_y) = (vec![0.; n * 4], vec![0.; n * 4]);
		let (mut c_lat, mut c_lng) = (vec![0.; n], vec![0.; n]);
		let (mut lat0, mut lng0) = (0., 0.);
		for i in 0..n {
			let (mut sx, mut sy) = (0., 0.);
			for k in 0..4 {
				let (lng, lat) = (payload.ring[i * 8 + k * 2], payload.ring[i * 8 + k * 2 + 1]);
				ring_x[i * 4 + k] = world_x(lng);
				ring_y[i * 4 + k] = world_y(lat);
				sx += lng;
				sy += lat;
			}
			c_lng[i] = sx / 4.;
			c_lat[i] = sy / 4.;
			lat0 += c_lat[i];
			lng0 += c_lng[i];
		}
		lat0 /= n as f64;
		lng0 /= n as f64;
		let m_per_lng = std::f64::consts::PI / 180. * R * lat0.to_radians().cos();
		let m_per_lat = std::f64::consts::PI / 180. * R;
		let local = |lat: f64, lng: f64| ((lng - lng0) * m_per_lng, (lat - lat0) * m_per_lat);

		let (mut mx, mut my) = (vec![0.; n], vec![0.; n]);
		for i in 0..n {
			(mx[i], my[i]) = local(c_lat[i], c_lng[i]);
		}
		let comps = payload
			.pois
			.iter()
			.map(|p| {
				let tier = payload
					.tiers
					.iter()
					.position(|t| t.name == p.poi.tier)
					.ok_or_else(|| eyre::eyre!("competitor {:?} is in tier {:?}, which the payload does not declare", p.poi.name, p.poi.tier))?;
				let (mx, my) = local(p.poi.lat, p.poi.lng);
				Ok(Comp { mx, my, w: p.w, tier })
			})
			.collect::<Result<Vec<_>>>()?;

		Ok(Self {
			payload,
			ring_x,
			ring_y,
			c_lat,
			c_lng,
			mx,
			my,
			lat0,
			lng0,
			m_per_lng,
			m_per_lat,
			comps,
		})
	}

	pub fn n(&self) -> usize {
		self.payload.place.len()
	}

	/// The tier controls at their opening positions.
	pub fn tier_states(&self) -> Vec<TierState> {
		self.payload
			.tiers
			.iter()
			.map(|t| TierState {
				name: t.name.clone(),
				weight: t.weight,
				show: true,
			})
			.collect()
	}

	pub fn to_local(&self, lat: f64, lng: f64) -> (f64, f64) {
		((lng - self.lng0) * self.m_per_lng, (lat - self.lat0) * self.m_per_lat)
	}

	/// Σ over competitors of weight × exp(−distance/λ).
	pub fn pressure(&self, lambda: f64, tiers: &[TierState]) -> Vec<f64> {
		let cut2 = (CUTOFF * lambda) * (CUTOFF * lambda);
		let mut press = vec![0.; self.n()];
		for c in &self.comps {
			let t = &tiers[c.tier];
			if !t.show {
				continue;
			}
			let base = c.w * t.weight;
			if base <= 0. {
				continue;
			}
			for (i, p) in press.iter_mut().enumerate() {
				let (dx, dy) = (self.mx[i] - c.mx, self.my[i] - c.my);
				let d2 = dx * dx + dy * dy;
				if d2 > cut2 {
					continue;
				}
				*p += base * (-d2.sqrt() / lambda).exp();
			}
		}
		press
	}

	pub fn unmet(&self, press: &[f64]) -> Vec<f64> {
		self.payload.demand.iter().zip(press).map(|(d, p)| d / (1. + p)).collect()
	}

	/// The three that move under the sliders, then whatever the study asked for.
	pub fn layers(&self) -> Vec<LayerSpec> {
		let mut out = vec![
			LayerSpec {
				name: "Underserved demand  ★".to_owned(),
				note: "Demand in the cell, divided down by how much competition already reaches it. This is the layer to shop for locations on.".to_owned(),
				linear: false,
				source: LayerRef::Unmet,
			},
			LayerSpec {
				name: "Competitor pressure".to_owned(),
				note: "Σ over competitors of weight × exp(−distance/λ). High = already saturated.".to_owned(),
				linear: false,
				source: LayerRef::Pressure,
			},
			LayerSpec {
				name: "Demand".to_owned(),
				note: self.payload.demand_note.clone(),
				linear: false,
				source: LayerRef::Demand,
			},
		];
		out.extend(self.payload.layers.iter().enumerate().map(|(i, l)| LayerSpec {
			name: l.name.clone(),
			note: l.note.clone(),
			linear: matches!(l.scale, Scale::Linear),
			source: LayerRef::Study(i),
		}));
		out
	}

	pub fn values<'a>(&'a self, source: LayerRef, press: &'a [f64], unmet: &'a [f64]) -> &'a [f64] {
		match source {
			LayerRef::Unmet => unmet,
			LayerRef::Pressure => press,
			LayerRef::Demand => &self.payload.demand,
			LayerRef::Study(i) => &self.payload.layers[i].values,
		}
	}

	/// Huff-style: a candidate captures each cell's demand in proportion to its own distance-decayed
	/// pull against all competition already reaching that cell.
	pub fn capture_at(&self, mx: f64, my: f64, lambda: f64, press: &[f64]) -> f64 {
		let cut2 = (CUTOFF * lambda) * (CUTOFF * lambda);
		let mut cap = 0.;
		for (i, p) in press.iter().enumerate() {
			let (dx, dy) = (self.mx[i] - mx, self.my[i] - my);
			let d2 = dx * dx + dy * dy;
			if d2 > cut2 {
				continue;
			}
			let pull = (-d2.sqrt() / lambda).exp();
			cap += self.payload.demand[i] * pull / (pull + p);
		}
		cap
	}

	/// Demand within 1 km and within 3 km.
	pub fn demand_within(&self, mx: f64, my: f64) -> (f64, f64) {
		let (mut d1, mut d3) = (0., 0.);
		for i in 0..self.n() {
			let (dx, dy) = (self.mx[i] - mx, self.my[i] - my);
			let d2 = dx * dx + dy * dy;
			if d2 > 9e6 {
				continue;
			}
			d3 += self.payload.demand[i];
			if d2 < 1e6 {
				d1 += self.payload.demand[i];
			}
		}
		(d1, d3)
	}

	/// Closest competitor and its distance in metres, over the whole inventory or one tier. Tier
	/// visibility does not enter: what is on the ground is on the ground.
	pub fn nearest(&self, mx: f64, my: f64, tier: Option<&str>) -> Option<(&PoiOut, f64)> {
		self.payload
			.pois
			.iter()
			.zip(&self.comps)
			.filter(|(p, _)| tier.is_none_or(|t| p.poi.tier == t))
			.map(|(p, c)| (p, (c.mx - mx).hypot(c.my - my)))
			.min_by(|a, b| a.1.total_cmp(&b.1))
	}

	pub fn within(&self, mx: f64, my: f64, r: f64, tier: Option<&str>) -> usize {
		self.payload
			.pois
			.iter()
			.zip(&self.comps)
			.filter(|(p, c)| tier.is_none_or(|t| p.poi.tier == t) && (c.mx - mx).hypot(c.my - my) < r)
			.count()
	}

	pub fn nearest_cell(&self, mx: f64, my: f64) -> usize {
		(0..self.n())
			.min_by(|&a, &b| (self.mx[a] - mx).hypot(self.my[a] - my).total_cmp(&(self.mx[b] - mx).hypot(self.my[b] - my)))
			.expect("the model rejects an empty grid")
	}

	/// The cell under the cursor, on the same ~120 m tolerance the map used. `None` off the grid.
	pub fn cell_at(&self, lat: f64, lng: f64, shown: &[u8]) -> Option<usize> {
		let d_lat = 0.0011;
		let d_lng = 0.0011 / lat.to_radians().cos();
		(0..self.n()).find(|&i| shown[i] != 0 && (self.c_lat[i] - lat).abs() < d_lat && (self.c_lng[i] - lng).abs() < d_lng)
	}

	pub fn tooltip(&self, i: usize, press: &[f64], unmet: &[f64]) -> String {
		let mut s = self.payload.place[i].clone();
		for l in &self.payload.layers {
			s.push_str(&format!("\n{} {}", l.name, fmt(l.values[i])));
		}
		s.push_str(&format!("\ndemand {} · pressure {:.2} · unmet {}", fmt(self.payload.demand[i]), press[i], fmt(unmet[i])));
		if self.payload.imputed[i] == 1 {
			s.push_str("\n(imputed cell)");
		}
		s
	}

	/// Greedy top-N sweep over cell centroids, one candidate per 400 m and picks kept 1.5 km apart.
	/// Returns the picks and how many cells were in the running.
	pub fn rank_sites(&self, lambda: f64, press: &[f64], take: usize) -> (Vec<Ranked>, usize) {
		// one candidate per 400 m: 200 m spacing buys nothing and quadruples the cost
		let mut seen = std::collections::HashSet::new();
		let mut cand = Vec::new();
		for i in 0..self.n() {
			if self.payload.demand[i] <= 0. {
				continue;
			}
			let key = ((self.mx[i] / 400. + 0.5).floor() as i64, (self.my[i] / 400. + 0.5).floor() as i64);
			if seen.insert(key) {
				cand.push(i);
			}
		}
		let mut scored: Vec<(usize, f64)> = cand.iter().map(|&i| (i, self.capture_at(self.mx[i], self.my[i], lambda, press))).collect();
		scored.sort_by(|a, b| b.1.total_cmp(&a.1));

		let mut picked: Vec<Ranked> = Vec::new();
		for (i, score) in scored {
			if picked.iter().all(|p| (self.mx[p.cell] - self.mx[i]).hypot(self.my[p.cell] - self.my[i]) > 1500.) {
				picked.push(Ranked {
					cell: i,
					lat: self.c_lat[i],
					lng: self.c_lng[i],
					score,
				});
			}
			if picked.len() == take {
				break;
			}
		}
		(picked, cand.len())
	}

	pub fn site_report(&self, lat: f64, lng: f64, label: Option<&str>, lambda: f64, press: &[f64], tiers: &[TierState]) -> Report {
		let (mx, my) = self.to_local(lat, lng);
		let (d1, d3) = self.demand_within(mx, my);
		let any = self.nearest(mx, my, None);
		let km = |d: Option<f64>| d.map_or_else(|| "–".to_owned(), |d| format!("{:.2} km", d / 1000.));

		let mut rows = vec![
			row("Capture score", fmt(self.capture_at(mx, my, lambda, press))),
			row("Demand ≤1 / ≤3 km", format!("{} / {}", fmt(d1), fmt(d3))),
		];
		for t in tiers {
			let near = self.nearest(mx, my, Some(&t.name));
			rows.push(Row {
				label: format!("Nearest {}", t.name),
				sub: Some(near.map_or_else(|| "–".to_owned(), |(p, _)| p.poi.name.chars().take(28).collect())),
				value: km(near.map(|(_, d)| d)),
			});
			rows.push(row(
				format!("{} ≤2 / ≤5 km", t.name),
				format!("{} / {}", self.within(mx, my, 2000., Some(&t.name)), self.within(mx, my, 5000., Some(&t.name))),
			));
		}
		rows.push(row("Nearest competitor, any", km(any.map(|(_, d)| d))));
		rows.push(row("All competitors ≤2 km", self.within(mx, my, 2000., None).to_string()));
		rows.push(row("Coordinates", format!("{lat:.5}, {lng:.5}")));

		Report {
			title: match label {
				Some(l) => format!("{l} — {}", self.payload.place[self.nearest_cell(mx, my)]),
				None => self.payload.place[self.nearest_cell(mx, my)].clone(),
			},
			rows,
			note: "Capture = Σ demand × pull/(pull + existing pressure). Only meaningful when comparing candidates against each other; the number has no unit.".to_owned(),
		}
	}

	pub fn compare(&self, candidates: &[Candidate], lambda: f64, press: &[f64]) -> Report {
		struct Line {
			name: String,
			cap: f64,
			d1: f64,
			d3: f64,
			near: Option<f64>,
			n2: usize,
		}
		let lines: Vec<Line> = candidates
			.iter()
			.map(|c| {
				let (mx, my) = self.to_local(c.at[0], c.at[1]);
				let (d1, d3) = self.demand_within(mx, my);
				Line {
					name: c.name.clone(),
					cap: self.capture_at(mx, my, lambda, press),
					d1,
					d3,
					near: self.nearest(mx, my, None).map(|(_, d)| d),
					n2: self.within(mx, my, 2000., None),
				}
			})
			.collect();
		let best = lines.iter().map(|l| l.cap).fold(f64::NEG_INFINITY, f64::max);

		let mut rows = vec![heading("Capture score")];
		rows.extend(lines.iter().map(|l| row(&l.name, format!("{} · {:.0}%", fmt(l.cap), 100. * l.cap / best))));
		rows.push(heading("Demand ≤1 / ≤3 km"));
		rows.extend(lines.iter().map(|l| row(&l.name, format!("{} / {}", fmt(l.d1), fmt(l.d3)))));
		rows.push(heading("Nearest competitor · ≤2 km"));
		rows.extend(lines.iter().map(|l| {
			row(
				&l.name,
				match l.near {
					Some(d) => format!("{:.2} km · {}", d / 1000., l.n2),
					None => format!("– · {}", l.n2),
				},
			)
		}));

		Report {
			title: format!("Candidates · λ={:.1} km", lambda / 1000.),
			rows,
			note: "Percentages are against the best candidate here, not against the best site in the area — use \"Rank top 10 sites\" for that.".to_owned(),
		}
	}
}

pub fn ramp(t: f64) -> [f64; 3] {
	let t = t.clamp(0., 1.) * (TURBO.len() - 1) as f64;
	let i = (t.floor() as usize).min(TURBO.len() - 2);
	let f = t - i as f64;
	let (a, b) = (TURBO[i], TURBO[i + 1]);
	[a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]
}
pub struct Colorised {
	/// `n * 3` RGB. Only the entries `shown` marks are meaningful.
	pub colors: Vec<u8>,
	pub shown: Vec<u8>,
	/// The legend's five labels, at 0/25/50/75/100 %.
	pub ticks: [f64; 5],
	pub count: usize,
}
pub fn colorise(values: &[f64], imputed: &[u8], hide_imputed: bool, linear: bool) -> Colorised {
	let n = values.len();
	let mut shown = vec![0u8; n];
	let mut idx = Vec::new();
	for i in 0..n {
		shown[i] = u8::from(!(hide_imputed && imputed[i] == 1) && values[i] > 0.);
		if shown[i] == 1 {
			idx.push(i);
		}
	}
	let mut sorted: Vec<f64> = idx.iter().map(|&i| values[i]).collect();
	sorted.sort_by(f64::total_cmp);
	let count = sorted.len();
	let (lo, hi) = if count > 0 { (sorted[0], sorted[count - 1]) } else { (0., 1.) };

	let mut colors = vec![0u8; n * 3];
	for &i in &idx {
		let t = if linear {
			(values[i] - lo) / if hi - lo == 0. { 1. } else { hi - lo }
		} else {
			// rank within the shown cells: `partition_point` is the JS binary search verbatim
			sorted.partition_point(|&s| s < values[i]) as f64 / if count > 1 { (count - 1) as f64 } else { 1. }
		};
		let c = ramp(t);
		colors[i * 3] = c[0] as u8;
		colors[i * 3 + 1] = c[1] as u8;
		colors[i * 3 + 2] = c[2] as u8;
	}
	let at = |f: f64| {
		if count == 0 {
			0.
		} else if linear {
			lo + f * (hi - lo)
		} else {
			sorted[(f * (count - 1) as f64).floor() as usize]
		}
	};
	Colorised {
		colors,
		shown,
		ticks: [at(0.), at(0.25), at(0.5), at(0.75), at(1.)],
		count,
	}
}
/// Three significant-ish digits with an SI-ish suffix. Report and legend both read it, so a change
/// here moves every number on the page at once.
pub fn fmt(n: f64) -> String {
	if n >= 1e6 {
		format!("{:.2}M", n / 1e6)
	} else if n >= 1e4 {
		format!("{:.0}k", n / 1000.)
	} else if n >= 1000. {
		format!("{:.1}k", n / 1000.)
	} else if n >= 10. {
		format!("{n:.0}")
	} else {
		format!("{n:.2}")
	}
}
/// One competitor, reduced to what the model reads.
struct Comp {
	mx: f64,
	my: f64,
	/// `poi_weight` at this shop, before the tier multiplier.
	w: f64,
	tier: usize,
}

fn row(label: impl Into<String>, value: impl Into<String>) -> Row {
	Row {
		label: label.into(),
		sub: None,
		value: value.into(),
	}
}

fn heading(label: impl Into<String>) -> Row {
	Row {
		label: label.into(),
		sub: None,
		value: String::new(),
	}
}
