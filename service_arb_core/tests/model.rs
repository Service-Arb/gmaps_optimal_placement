//! The numbers the map used to compute in the browser, pinned against the Clermont study.
//!
//! `clermont_payload.json` is one `service_arb map examples/clermont_detailing/config.nix` — the
//! billed half of the pipeline, frozen, so this runs with no network and no key.
use service_arb_core::{
	Model, Payload,
	model::{Report, TierState, colorise, fmt},
};

fn model() -> Model {
	let payload: Payload = serde_json::from_str(include_str!("clermont_payload.json")).unwrap();
	Model::try_new(payload).unwrap()
}

fn sum(v: &[f64]) -> f64 {
	v.iter().sum()
}

fn nonzero(v: &[f64]) -> usize {
	v.iter().filter(|&&x| x > 0.).count()
}

fn render(r: &Report) -> String {
	let mut s = format!("{}\n", r.title);
	for row in &r.rows {
		s.push_str(&format!("  {:<28} {}\n", row.label, row.value));
		if let Some(sub) = &row.sub {
			s.push_str(&format!("    {sub}\n"));
		}
	}
	s
}

#[test]
fn state_matches_the_study() {
	let m = model();
	let tiers = m.tier_states();
	let press = m.pressure(m.payload.lambda_m, &tiers);
	let unmet = m.unmet(&press);
	let c = colorise(&unmet, &m.payload.imputed, false, false);

	insta::assert_snapshot!(format!(
		"cells {}\ncompetitors {}\nimputed {}\nshown {}\npressure nonzero {}\ndemand nonzero {}\ndemand total {:.1}\nunmet total {:.1}\npressure total {:.6}",
		m.n(),
		m.payload.pois.len(),
		m.payload.imputed.iter().filter(|&&i| i == 1).count(),
		c.count,
		nonzero(&press),
		nonzero(&m.payload.demand),
		sum(&m.payload.demand),
		sum(&unmet),
		sum(&press),
	), @"
	cells 10446
	competitors 140
	imputed 6532
	shown 10446
	pressure nonzero 8576
	demand nonzero 10446
	demand total 346002.4
	unmet total 147075.7
	pressure total 14033.335823
	");
}

/// The three sensitivities `smoke.js` drove the page for: dropping a tier lowers pressure, a wider
/// catchment raises it, and recompute is a function of its inputs alone.
#[test]
fn pressure_moves_with_the_controls() {
	let m = model();
	let base = m.tier_states();
	let mut no_wash = base.clone();
	no_wash.iter_mut().find(|t| t.name == "wash").unwrap().show = false;

	let p_base = sum(&m.pressure(m.payload.lambda_m, &base));
	let p_no_wash = sum(&m.pressure(m.payload.lambda_m, &no_wash));
	let p_wide = sum(&m.pressure(4000., &base));
	let p_again = sum(&m.pressure(m.payload.lambda_m, &base));

	assert!(p_no_wash < p_base, "dropping washes must lower pressure");
	assert!(p_wide > p_base, "wider λ must raise pressure");
	assert_eq!(p_base, p_again, "recompute is deterministic");

	insta::assert_snapshot!(format!("base {p_base:.6}\nno wash {p_no_wash:.6}\nlambda 4000 {p_wide:.6}"), @"
	base 14033.335823
	no wash 7408.933201
	lambda 4000 45012.247947
	");
}

#[test]
fn top_ten_sites() {
	let m = model();
	let tiers = m.tier_states();
	let press = m.pressure(m.payload.lambda_m, &tiers);
	let (picked, pool) = m.rank_sites(m.payload.lambda_m, &press, 10);

	let mut s = format!("{pool} candidate cells\n");
	for (k, r) in picked.iter().enumerate() {
		s.push_str(&format!("{:>2}. {:<28} {:>6}  {:.5}, {:.5}\n", k + 1, m.payload.place[r.cell], fmt(r.score), r.lat, r.lng));
	}
	insta::assert_snapshot!(s, @"
	4617 candidate cells
	 1. Chamalières                    9.0k  45.77289, 3.06944
	 2. Châtel-Guyon                   8.9k  45.91457, 3.07977
	 3. Ceyrat                         8.6k  45.75625, 3.06382
	 4. Saint-Bonnet-près-Riom         8.4k  45.92441, 3.11991
	 5. Ceyrat                         8.2k  45.74140, 3.05798
	 6. Châtel-Guyon                   8.2k  45.92432, 3.06300
	 7. Riom                           8.1k  45.90678, 3.09888
	 8. Saint-Bonnet-près-Riom         8.1k  45.92650, 3.09636
	 9. Royat                          8.1k  45.76617, 3.04965
	10. Clermont-Ferrand               7.9k  45.76296, 3.08360
	");
}

#[test]
fn site_report_downtown() {
	let m = model();
	let tiers = m.tier_states();
	let press = m.pressure(m.payload.lambda_m, &tiers);
	insta::assert_snapshot!(render(&m.site_report(45.7797, 3.0863, None, m.payload.lambda_m, &press, &tiers)), @"
	Clermont-Ferrand
	  Capture score                7.4k
	  Demand ≤1 / ≤3 km            16k / 81k
	  Nearest detail               0.19 km
	    Clermont Nettoyage Auto et P
	  detail ≤2 / ≤5 km            6 / 17
	  Nearest wash                 0.59 km
	    Lavage auto EXEPXION
	  wash ≤2 / ≤5 km              7 / 29
	  Nearest competitor, any      0.19 km
	  All competitors ≤2 km        13
	  Coordinates                  45.77970, 3.08630
	");
}

#[test]
fn candidates_compare() {
	let m = model();
	let tiers = m.tier_states();
	let press = m.pressure(m.payload.lambda_m, &tiers);
	insta::assert_snapshot!(render(&m.compare(&m.payload.candidates, m.payload.lambda_m, &press)), @"
	Candidates · λ=2.0 km
	  Capture score                
	  VifNet                       8.8k · 100%
	  Demand ≤1 / ≤3 km            
	  VifNet                       15k / 70k
	  Nearest competitor · ≤2 km   
	  VifNet                       1.01 km · 9
	");
}

/// Every layer colours cells, and `hide imputed` takes exactly the imputed ones out.
#[test]
fn every_layer_colours() {
	let m = model();
	let tiers = m.tier_states();
	let press = m.pressure(m.payload.lambda_m, &tiers);
	let unmet = m.unmet(&press);

	let mut s = String::new();
	for spec in m.layers() {
		let v = m.values(spec.source, &press, &unmet);
		let shown = colorise(v, &m.payload.imputed, false, spec.linear);
		let hidden = colorise(v, &m.payload.imputed, true, spec.linear);
		s.push_str(&format!(
			"{:<28} {:>5} shown, {:>5} without imputed, ticks {}\n",
			spec.name,
			shown.count,
			hidden.count,
			shown.ticks.map(fmt).join(" · ")
		));
	}
	insta::assert_snapshot!(s, @"
	Underserved demand  ★        10446 shown,  3914 without imputed, ticks 0.02 · 2.60 · 8.29 · 20 · 185
	Competitor pressure           8576 shown,  3692 without imputed, ticks 0.01 · 0.21 · 0.62 · 2.09 · 10
	Demand                       10446 shown,  3914 without imputed, ticks 0.23 · 3.65 · 13 · 45 · 786
	Population                   10446 shown,  3914 without imputed, ticks 1.00 · 4.00 · 15 · 50 · 1.3k
	Households                   10446 shown,  3914 without imputed, ticks 0.30 · 1.90 · 6.20 · 21 · 828
	Estimated cars               10446 shown,  3914 without imputed, ticks 0.33 · 2.79 · 9.46 · 31 · 704
	Standard of living (€/yr)    10446 shown,  3914 without imputed, ticks 11k · 19k · 27k · 35k · 43k
	Households in houses         10398 shown,  3869 without imputed, ticks 0.10 · 1.70 · 5.70 · 17 · 175
	");
}

/// The tooltip is the only place the per-cell numbers are ever read next to each other.
#[test]
fn tooltip_over_a_cell() {
	let m = model();
	let tiers: Vec<TierState> = m.tier_states();
	let press = m.pressure(m.payload.lambda_m, &tiers);
	let unmet = m.unmet(&press);
	let shown = colorise(&unmet, &m.payload.imputed, false, false).shown;
	let hit = m.cell_at(45.7797, 3.0863, &shown).expect("the study centre is on the grid");
	insta::assert_snapshot!(m.tooltip(hit, &press, &unmet), @"
	Clermont-Ferrand
	Population 424
	Households 256
	Estimated cars 219
	Standard of living (€/yr) 29k
	Households in houses 2.00
	demand 339 · pressure 8.29 · unmet 36
	");
}
