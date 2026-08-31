//! Statistical grids published as one CSV of EPSG:3035 cells.
//!
//! Both catalogued sources encode the cell corner in its id, so the whole difference between them
//! is the [`Archive`] table below: where the file is, which columns are text, and which one carries
//! provenance.
use std::collections::{BTreeSet, HashMap};

use eyre::{Result, WrapErr, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_arb_core::{
	Reproject,
	grid::{Bbox, Cell, CellId, Grid},
};

use crate::work::Work;

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub enum GridSource {
	/// INSEE Filosofi — France, 200 m, households / housing type / standard of living.
	#[serde(rename = "insee_filosofi_200m")]
	InseeFilosofi200m,
	/// Eurostat census grid — EU, 1 km, head counts by age, sex, employment and origin.
	#[serde(rename = "geostat_1km")]
	Geostat1km,
}
impl GridSource {
	fn archive(self, vintage: u16) -> Result<Archive> {
		Ok(match (self, vintage) {
			(Self::InseeFilosofi200m, 2021) => Archive {
				url: "https://www.insee.fr/fr/statistiques/fichier/8735162/Filosofi2021_carreaux_200m_csv.zip",
				member: "carreaux_200m_met.csv",
				id_col: "idcar_200m",
				place_col: "lcog_geo",
				imputed_col: Some("i_est_200"),
				text_cols: &["idcar_200m", "idcar_1km", "idcar_nat", "lcog_geo"],
				res_m: 200,
			},
			(Self::Geostat1km, 2021) => Archive {
				url: "https://gisco-services.ec.europa.eu/census/2021/Eurostat_Census-GRID_2021_V3.zip",
				member: "Eurostat_Census-GRID_2021_V3/ESTAT_Census_2021_V3.csv",
				id_col: "GRD_ID",
				place_col: "CNTR_ID",
				imputed_col: None,
				text_cols: &["GRD_ID", "CNTR_ID"],
				res_m: 1000,
			},
			(s, v) => bail!("no {s:?} archive for vintage {v}"),
		})
	}
}

pub fn load(source: GridSource, vintage: u16, bbox: Bbox, work: &Work) -> Result<Grid> {
	let a = source.archive(vintage)?;
	let proj = Reproject::try_new()?;
	let [x0, y0, x1, y1] = bbox.to_laea(&proj)?;

	let path = work.archive(a.url)?;
	let file = std::fs::File::open(&path).wrap_err_with(|| format!("opening {}", path.display()))?;
	let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).wrap_err_with(|| format!("{} is not a zip", path.display()))?;
	let member = zip.by_name(a.member).wrap_err_with(|| format!("{} has no member {:?}", path.display(), a.member))?;
	let mut rdr = csv::ReaderBuilder::new().from_reader(std::io::BufReader::with_capacity(1 << 20, member));

	let header: Vec<String> = rdr.headers()?.iter().map(str::to_owned).collect();
	let at = |name: &str| {
		header
			.iter()
			.position(|h| h == name)
			.ok_or_else(|| eyre::eyre!("{:?} has no column {name:?}; header is {}", a.member, header.join(", ")))
	};
	let (i_id, i_place) = (at(a.id_col)?, at(a.place_col)?);
	let i_imputed = a.imputed_col.map(&at).transpose()?;
	for t in a.text_cols {
		at(t)?;
	}
	let numeric: Vec<usize> = (0..header.len()).filter(|i| !a.text_cols.contains(&header[*i].as_str())).collect();

	let mut grid = Grid::default();
	let (mut scanned, mut row) = (0usize, csv::StringRecord::new());
	let mut values: Vec<(&str, f64)> = Vec::with_capacity(numeric.len());
	while rdr.read_record(&mut row)? {
		scanned += 1;
		let id = &row[i_id];
		let cell = CellId::parse(id).wrap_err_with(|| format!("row {scanned} of {:?}", a.member))?;
		ensure!(cell.res_m == a.res_m, "{id} is a {} m cell, {:?} is meant to be {} m", cell.res_m, a.member, a.res_m);
		let (e, n) = (cell.east as f64, cell.north as f64);
		if e < x0 || e > x1 || n < y0 || n > y1 {
			continue;
		}
		values.clear();
		for &i in &numeric {
			let raw = &row[i];
			let v: f64 = raw.parse().wrap_err_with(|| format!("column {:?} of cell {id} is {raw:?}, not a number", header[i]))?;
			values.push((&header[i], v));
		}
		let imputed = match i_imputed {
			Some(i) => match &row[i] {
				"1" => true,
				"0" => false,
				other => bail!("provenance flag {:?} of cell {id} is {other:?}, expected 0 or 1", a.imputed_col),
			},
			None => false,
		};
		let cell = Cell {
			id: id.to_owned(),
			place: row[i_place].split(',').next().unwrap_or_default().to_owned(),
			ring: cell.ring(&proj)?,
			imputed,
		};
		grid.push(cell, &values)?;
	}
	eprintln!("{:?}: scanned {scanned} cells, kept {}", a.member, grid.len());
	ensure!(!grid.is_empty(), "no {source:?} cell falls inside {bbox:?}");

	if source == GridSource::InseeFilosofi200m {
		name_communes(&mut grid, work)?;
	}
	Ok(grid)
}
struct Archive {
	url: &'static str,
	member: &'static str,
	id_col: &'static str,
	place_col: &'static str,
	/// 1 where the row is modelled rather than observed.
	imputed_col: Option<&'static str>,
	/// Everything else in the header must parse as a number, for every cell.
	text_cols: &'static [&'static str],
	res_m: u32,
}

/// INSEE's `lcog_geo` is a commune code. The map wants the name.
fn name_communes(grid: &mut Grid, work: &Work) -> Result<()> {
	let mut names: HashMap<String, String> = HashMap::new();
	let depts: BTreeSet<&str> = grid.cells.iter().map(|c| &c.place[..2]).collect();
	for d in depts {
		let url = format!("https://geo.api.gouv.fr/departements/{d}/communes?fields=nom");
		for c in work
			.cached_get(&url, &format!("communes_{d}.json"))?
			.as_array()
			.ok_or_else(|| eyre::eyre!("{url} did not return an array"))?
		{
			let (code, nom) = (c["code"].as_str(), c["nom"].as_str());
			let (Some(code), Some(nom)) = (code, nom) else {
				bail!("{url} returned a commune without code and nom: {c}")
			};
			names.insert(code.to_owned(), nom.to_owned());
		}
	}
	// A grid vintage is fixed; the commune list is current. Codes retired by a merger since keep
	// their code as the label — this names a cell, it does not feed the model.
	let mut retired = BTreeSet::new();
	for cell in &mut grid.cells {
		match names.get(&cell.place) {
			Some(name) => cell.place = name.clone(),
			None => {
				retired.insert(cell.place.clone());
			}
		}
	}
	if !retired.is_empty() {
		eprintln!("communes: {} code(s) no longer current, shown as codes: {retired:?}", retired.len());
	}
	Ok(())
}
