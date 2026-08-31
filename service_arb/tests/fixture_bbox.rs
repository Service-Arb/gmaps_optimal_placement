//! A few hundred cells of the real INSEE archive, through the real study's expressions. Snapshots
//! the numbers a model change would move, so it moves them visibly.
//!
//! Reads the bulk archive out of the workspace work dir, downloading it once if absent. No POI call
//! is made: [`Study::cells`] stops before the billed half.
use std::path::Path;

use service_arb::{Study, config::Area};
use service_arb_core::grid::Bbox;
use service_arb_sources::Work;

#[test]
fn clermont_centre() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("crate lives in the workspace");
	let mut study: Study = service_arb::load(&root.join("examples/clermont_detailing/config.nix")).unwrap();
	study.area = Area {
		bbox: Bbox {
			lat: [45.770, 45.790],
			lon: [3.050, 3.090],
		},
		..study.area
	};

	let c = study.cells(&Work::at(root.join("tmp/geo"))).unwrap();
	let sum = |v: &[f64]| v.iter().sum::<f64>();
	let mut lines = vec![
		format!("cells           {}", c.grid.len()),
		format!("imputed         {}", c.grid.cells.iter().filter(|x| x.imputed).count()),
		format!("columns         {}", c.grid.columns.len()),
		format!("demand total    {:.1}", sum(&c.demand)),
	];
	lines.extend(c.layers.iter().map(|l| format!("{:<15} {:.1}", l.name, sum(&l.values))));
	insta::assert_snapshot!(lines.join("\n"), @"
	cells           196
	imputed         9
	columns         32
	demand total    36501.2
	Population      49327.0
	Households      29006.8
	Estimated cars  27082.0
	Standard of living (€/yr) 5571857.5
	Households in houses 3466.1
	");
}
