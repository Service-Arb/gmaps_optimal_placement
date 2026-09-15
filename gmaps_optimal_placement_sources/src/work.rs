//! The untracked work dir: bulk archives and API responses, so a rerun costs nothing.
use std::{
	cell::Cell,
	fs,
	io::Read,
	path::{Path, PathBuf},
};

use eyre::{Result, WrapErr, bail};
use sha2::{Digest, Sha256};

pub struct Work {
	dir: PathBuf,
	billed: Cell<usize>,
	/// Configured rather than `ureq::post`: the default turns a 4xx into an error before the body is
	/// read, and a Google error body is the only thing that says which quota ran out.
	agent: ureq::Agent,
}

impl Work {
	/// `GMAPS_OPTIMAL_PLACEMENT_WORK`, else `./tmp/geo` — gitignored, and where the prototype already put its
	/// downloads.
	pub fn from_env() -> Self {
		Self::at(std::env::var_os("GMAPS_OPTIMAL_PLACEMENT_WORK").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("tmp/geo")))
	}

	pub fn at(dir: impl Into<PathBuf>) -> Self {
		Self {
			dir: dir.into(),
			billed: Cell::new(0),
			agent: ureq::Agent::config_builder().http_status_as_error(false).build().new_agent(),
		}
	}

	pub fn path(&self) -> &Path {
		&self.dir
	}

	/// Cache misses since this `Work` was made — the calls a run actually paid for.
	pub fn billed(&self) -> usize {
		self.billed.get()
	}

	fn dir(&self, sub: &str) -> Result<PathBuf> {
		let p = self.dir.join(sub);
		fs::create_dir_all(&p).wrap_err_with(|| format!("creating {}", p.display()))?;
		Ok(p)
	}

	/// A bulk archive, downloaded once. Named after the URL's last segment so the file on disk says
	/// what it is.
	pub fn archive(&self, url: &str) -> Result<PathBuf> {
		let name = url.rsplit('/').next().filter(|s| !s.is_empty()).ok_or_else(|| eyre::eyre!("no filename in {url}"))?;
		let dst = self.dir("data")?.join(name);
		if dst.exists() {
			return Ok(dst);
		}
		eprintln!("downloading {url}");
		let mut body = ureq::get(url).call().wrap_err_with(|| format!("GET {url}"))?.into_body().into_reader();
		let part = dst.with_extension("part");
		let mut out = fs::File::create(&part).wrap_err_with(|| format!("creating {}", part.display()))?;
		let n = std::io::copy(&mut body, &mut out)?;
		if n == 0 {
			bail!("{url} returned an empty body");
		}
		fs::rename(&part, &dst)?;
		eprintln!("  {} ({:.0} MB)", dst.display(), n as f64 / 1e6);
		Ok(dst)
	}

	/// A cached POST. The key is the request itself, so editing a query refetches and rerunning does
	/// not.
	///
	/// The key covers url and body but not headers, and a Places field mask is a header that changes
	/// the response shape. Two callers asking the same body under different masks would share one
	/// entry; today they do not, because they differ in the body as well.
	pub fn cached_post(&self, url: &str, body: &serde_json::Value, headers: &[(&str, &str)]) -> Result<serde_json::Value> {
		if let Some(hit) = self.cached(url, body)? {
			return Ok(hit);
		}
		let canonical = serde_json::to_string(body)?;
		let dst = self.post_path(url, &canonical)?;
		let mut req = self.agent.post(url).header("Content-Type", "application/json");
		for (k, v) in headers {
			req = req.header(*k, *v);
		}
		let mut res = req.send(canonical.as_bytes()).wrap_err_with(|| format!("POST {url}"))?;
		let status = res.status();
		let mut text = String::new();
		res.body_mut().as_reader().read_to_string(&mut text)?;
		// the body of a refusal says which quota or which field, and it is never written to the cache:
		// one cached 429 would answer for that request forever
		if !status.is_success() {
			bail!("POST {url} -> {status}\n{}", text.trim());
		}
		self.billed.set(self.billed.get() + 1);
		let json: serde_json::Value = serde_json::from_str(&text).wrap_err_with(|| format!("POST {url} returned non-JSON: {}", &text[..text.len().min(400)]))?;
		fs::write(&dst, &text)?;
		Ok(json)
	}

	/// The response to this exact request, if it has been made before. Reading the probe's orderings
	/// back this way is what keeps a refit from turning into a purchase.
	pub fn cached(&self, url: &str, body: &serde_json::Value) -> Result<Option<serde_json::Value>> {
		let dst = self.post_path(url, &serde_json::to_string(body)?)?;
		let Ok(s) = fs::read_to_string(&dst) else { return Ok(None) };
		serde_json::from_str(&s).map(Some).wrap_err_with(|| format!("cached response {} is not JSON", dst.display()))
	}

	fn post_path(&self, url: &str, canonical: &str) -> Result<PathBuf> {
		let key = hex(Sha256::digest(format!("{url}\n{canonical}").as_bytes()).as_slice());
		Ok(self.dir("data/places_cache")?.join(format!("{key}.json")))
	}

	/// A cached GET of a JSON document that never changes under us within a study.
	pub fn cached_get(&self, url: &str, dst: &str) -> Result<serde_json::Value> {
		let dst = self.dir("data")?.join(dst);
		if let Ok(s) = fs::read_to_string(&dst) {
			return serde_json::from_str(&s).wrap_err_with(|| format!("cached response {} is not JSON", dst.display()));
		}
		let text = ureq::get(url).call().wrap_err_with(|| format!("GET {url}"))?.into_body().read_to_string()?;
		let json = serde_json::from_str(&text).wrap_err_with(|| format!("GET {url} returned non-JSON: {}", &text[..text.len().min(400)]))?;
		fs::write(&dst, &text)?;
		Ok(json)
	}
}

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|b| format!("{b:02x}")).collect()
}
