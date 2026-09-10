//! The French cadastre: Etalab republishes the PCI as one gzipped GeoJSON per commune per layer, so
//! this caches by commune code rather than by request hash the way `Work::cached_post` does.
use std::{
	fs,
	io::Read,
	path::Path,
	sync::atomic::{AtomicU64, Ordering},
	thread,
	time::Duration,
};

use eyre::{Result, WrapErr, bail};
use gmaps_optimal_placement_sources::Work;

use crate::Tally;

/// The dated snapshot, not `latest`: a vintage that moved under us would move the counts with it.
const VINTAGE: &str = "2026-06-01";
/// Above this the mirror answers 404 to requests that are not real absences, which is also why a 404
/// is only believed after [`TRIES`] attempts.
const WORKERS: usize = 16;
const TRIES: u32 = 4;

/// One count per code, in the order given. `None` is a commune the cadastre publishes no vector layer
/// for — a gap, never a zero.
pub fn count(tally: Tally, codes: &[String], work: &Work) -> Result<Vec<Option<u32>>> {
	let dir = work.path().join("data").join(tally.layer());
	fs::create_dir_all(&dir).wrap_err_with(|| format!("creating {}", dir.display()))?;
	let (fetched, bytes) = (AtomicU64::new(0), AtomicU64::new(0));

	// Strided rather than chunked: a chunk is contiguous by department, and Paris takes as long as a
	// hundred villages.
	let mut done: Vec<(usize, Option<u32>)> = thread::scope(|s| {
		let workers: Vec<_> = (0..WORKERS.min(codes.len()))
			.map(|w| {
				let (dir, fetched, bytes) = (&dir, &fetched, &bytes);
				s.spawn(move || {
					let mut out = Vec::new();
					for (i, code) in codes.iter().enumerate().skip(w).step_by(WORKERS) {
						out.push((i, one(tally, code, dir, fetched, bytes)?));
					}
					Ok(out)
				})
			})
			.collect();
		workers
			.into_iter()
			.map(|h| h.join().map_err(|e| eyre::eyre!("cadastre worker panicked: {e:?}"))?)
			.collect::<Result<Vec<_>>>()
	})?
	.concat();
	done.sort_unstable_by_key(|(i, _)| *i);

	let (n, mb) = (fetched.load(Ordering::Relaxed), bytes.load(Ordering::Relaxed) as f64 / 1e6);
	eprintln!("cadastre {} {VINTAGE}: {} communes, {n} downloaded ({mb:.0} MB)", tally.layer(), codes.len());
	Ok(done.into_iter().map(|(_, v)| v).collect())
}

fn one(tally: Tally, code: &str, dir: &Path, fetched: &AtomicU64, bytes: &AtomicU64) -> Result<Option<u32>> {
	let absent = dir.join(format!("{code}.absent"));
	if absent.exists() {
		return Ok(None);
	}
	let gz = dir.join(format!("{code}.json.gz"));
	if !gz.exists() {
		let url = format!(
			"https://cadastre.data.gouv.fr/data/etalab-cadastre/{VINTAGE}/geojson/communes/{dep}/{code}/raw/pci-{code}-{layer}.json.gz",
			dep = &code[..2],
			layer = tally.layer(),
		);
		let mut body = Vec::new();
		for attempt in 1..=TRIES {
			match ureq::get(&url).call() {
				Ok(mut res) => {
					res.body_mut().as_reader().read_to_end(&mut body).wrap_err_with(|| format!("reading {url}"))?;
					break;
				}
				Err(ureq::Error::StatusCode(404)) if attempt == TRIES => {
					fs::write(&absent, "")?;
					return Ok(None);
				}
				Err(e) if attempt == TRIES => return Err(e).wrap_err_with(|| format!("GET {url}")),
				Err(_) => thread::sleep(Duration::from_secs(2 * attempt as u64)),
			}
		}
		if body.is_empty() {
			bail!("{url} returned an empty body");
		}
		fetched.fetch_add(1, Ordering::Relaxed);
		bytes.fetch_add(body.len() as u64, Ordering::Relaxed);
		// Written under a temporary name: a run killed mid-download must not leave a truncated file
		// that every later run trusts.
		let part = gz.with_extension("part");
		fs::write(&part, &body)?;
		fs::rename(&part, &gz)?;
	}

	let mut json = String::new();
	flate2::read::GzDecoder::new(fs::File::open(&gz)?)
		.read_to_string(&mut json)
		.wrap_err_with(|| format!("{} is not gzip, or not text", gz.display()))?;
	let doc: serde_json::Value = serde_json::from_str(&json).wrap_err_with(|| format!("{} is not JSON", gz.display()))?;
	let features = doc["features"].as_array().ok_or_else(|| eyre::eyre!("{} has no feature array", gz.display()))?;
	Ok(Some(features.iter().filter(|f| tally.keeps(&f["properties"])).count() as u32))
}

impl Tally {
	/// The PCI layer, which is also the cache directory under the work dir.
	pub(crate) fn layer(self) -> &'static str {
		match self {
			Self::Pool => "tsurf",
			Self::Building => "batiment",
			Self::Parcel => "parcelle",
		}
	}

	fn keeps(self, props: &serde_json::Value) -> bool {
		match self {
			// The surface layer draws more than pools, and 65 is the symbol every published French
			// pool count is built on. `SYM = 66` is a second, smaller, mostly recent polygon that a few
			// departments carry and those counts leave out; if it turns out to be pools too, every
			// commune in Ardennes, Ariège, Aveyron and Haute-Garonne is short.
			Self::Pool => props["SYM"] == "65",
			Self::Building | Self::Parcel => true,
		}
	}
}
