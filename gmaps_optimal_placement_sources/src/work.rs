//! The untracked work dir: bulk archives and API responses, so a rerun costs nothing.
//!
//! Nothing here expires. An answer past the age its kind allows is still served and its age is
//! reported, because the alternative — refetching — spends a day of quota to learn the same thing,
//! and the quota is the scarce side. What the ages buy is a map that says how old it is.
use std::{
	cell::{Cell, RefCell},
	fs,
	io::Read,
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use eyre::{Result, WrapErr, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// What an answer is about, which is what says how old it may get before the map says so.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
	/// Shop names, ratings, addresses. Shops open and close.
	Inventory,
	/// Who ranks where from each stratum — what `core::rank::COEF` is fitted on.
	Ordering,
	/// The commune list. Drifts only on mergers.
	Communes,
}

/// How old an answer of each kind may get before it is reported as old.
#[derive(Clone, Copy, Debug)]
pub struct Age {
	pub inventory: Duration,
	pub ordering: Duration,
	pub communes: Duration,
}
impl Age {
	pub const COMMUNES: Duration = YEAR;
	pub const INVENTORY: Duration = Duration::from_secs(7 * 86_400);
	pub const ORDERING: Duration = YEAR;

	fn of(&self, kind: Kind) -> Duration {
		match kind {
			Kind::Inventory => self.inventory,
			Kind::Ordering => self.ordering,
			Kind::Communes => self.communes,
		}
	}
}
impl Default for Age {
	fn default() -> Self {
		Self {
			inventory: Self::INVENTORY,
			ordering: Self::ORDERING,
			communes: Self::COMMUNES,
		}
	}
}
const YEAR: Duration = Duration::from_secs(365 * 86_400);
const DAY: u64 = 86_400;
/// Seconds past midnight UTC at which `SearchTextRequestPerDayPerProject` resets — midnight
/// US/Pacific, which moves by an hour twice a year. Corrected from any 429, which carries the
/// window's own start.
const WINDOW_PHASE: u64 = 7 * 3600;
/// The one quota a sweep runs out of.
const QUOTA: &str = "SearchTextRequestPerDayPerProject";

/// Oldest and median age of the answers one run served for a kind.
pub struct Aged {
	pub oldest: Duration,
	pub median: Duration,
	/// The oldest is past what the config allows. Reported, never acted on.
	pub stale: bool,
}
impl std::fmt::Display for Aged {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "oldest {:.1} d, median {:.1} d", days(self.oldest), days(self.median))
	}
}

/// How many searches something is about to need. A sweep cannot say exactly — a tile at the
/// result cap opens four more — and a lower bound announced as an exact number is a lie the
/// refusal would be built on.
#[derive(Clone, Copy, Debug)]
pub enum Need {
	Exact(usize),
	AtLeast(usize),
}
impl Need {
	fn count(self) -> usize {
		match self {
			Self::Exact(n) | Self::AtLeast(n) => n,
		}
	}
}
impl std::fmt::Display for Need {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Exact(n) => write!(f, "{n}"),
			Self::AtLeast(n) => write!(f, "at least {n}"),
		}
	}
}

pub struct Work {
	dir: PathBuf,
	billed: Cell<usize>,
	/// Configured rather than `ureq::post`: the default turns a 4xx into an error before the body is
	/// read, and a Google error body is the only thing that says which quota ran out.
	agent: ureq::Agent,
	age: Age,
	per_day: u32,
	/// Re-ask every billed request and overwrite what it answered. The only thing that spends on a
	/// question already answered; age alone never does.
	refresh: bool,
	/// Age of every cached answer this run served, by kind.
	served: RefCell<Vec<(Kind, Duration)>>,
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
			age: Age::default(),
			per_day: 100,
			refresh: false,
			served: RefCell::default(),
		}
	}

	/// The policy the CLI resolved out of its config. Plain values rather than the config type, so
	/// `_sources` stands alone and the wasm side never sees the settings crate.
	pub fn policy(mut self, age: Age, per_day: u32) -> Self {
		self.age = age;
		self.per_day = per_day;
		self
	}

	pub fn refresh(mut self, yes: bool) -> Self {
		self.refresh = yes;
		self
	}

	/// Whether an answer already on disk will be bought again — which is what a preflight has to
	/// count, and the read-only walk that counts it is deliberately not affected.
	pub fn refreshing(&self) -> bool {
		self.refresh
	}

	pub fn path(&self) -> &Path {
		&self.dir
	}

	/// Cache misses since this `Work` was made — the calls a run actually paid for.
	pub fn billed(&self) -> usize {
		self.billed.get()
	}

	/// Answers served for `kind`, paid or not — one per page, which is one unit of
	/// `SearchTextRequestPerDayPerProject` whatever the mask. `billed` counts only the misses, so
	/// this is what a keyless walk can report and that one cannot.
	pub(crate) fn calls(&self, kind: Kind) -> usize {
		self.served.borrow().iter().filter(|(k, _)| *k == kind).count()
	}

	/// What was served out of the cache for `kind`, and whether the oldest is past its age. `None`
	/// when this run served nothing of that kind.
	pub fn age(&self, kind: Kind) -> Option<Aged> {
		let mut seen: Vec<Duration> = self.served.borrow().iter().filter(|(k, _)| *k == kind).map(|(_, d)| *d).collect();
		if seen.is_empty() {
			return None;
		}
		seen.sort_unstable();
		let oldest = seen[seen.len() - 1];
		Some(Aged {
			oldest,
			median: seen[seen.len() / 2],
			stale: oldest > self.age.of(kind),
		})
	}

	/// Drop what has been recorded for `kind`. For a walk that read the cache to price a purchase
	/// rather than to answer with it: what it served is not what the map is painted from.
	pub fn forget(&self, kind: Kind) {
		self.served.borrow_mut().retain(|(k, _)| *k != kind);
	}

	/// An answer was served, and this is when it was written. Called by whoever knows what the call
	/// was about — the cache itself only sees a URL and a body.
	pub fn record(&self, kind: Kind, at: SystemTime) {
		// an mtime ahead of now is a clock that moved, and the answer is still not old
		let age = SystemTime::now().duration_since(at).unwrap_or(Duration::ZERO);
		self.served.borrow_mut().push((kind, age));
	}

	/// Refuses before anything spends. The message names the shortfall rather than the call that
	/// would have failed, because a sweep that dies a third of the way through has already burnt the
	/// window it needed.
	pub fn preflight(&self, what: &str, need: Need) -> Result<()> {
		let l = self.ledger()?;
		let left = self.per_day.saturating_sub(l.spent);
		if need.count() as u64 <= left as u64 {
			return Ok(());
		}
		let reset = l.window_start + DAY;
		bail!(
			"refusing to start: {what} needs {need} more searches,\nand {left} of today's {} remain (window resets {:02}:{:02} UTC).\nraise {QUOTA}, or rerun after the reset —\nnothing already answered is re-asked.",
			self.per_day,
			reset % DAY / 3600,
			reset % 3600 / 60,
		)
	}

	fn dir(&self, sub: &str) -> Result<PathBuf> {
		let p = self.dir.join(sub);
		fs::create_dir_all(&p).wrap_err_with(|| format!("creating {}", p.display()))?;
		Ok(p)
	}

	/// A bulk archive, downloaded once. Named after the URL's last segment so the file on disk says
	/// what it is.
	///
	/// No age: the vintage is in the URL, so re-fetching would return the same bytes, and asking for
	/// a different vintage asks for a different file.
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

	/// A cached POST, and when the answer was written. The key is the request itself, so editing a
	/// query refetches and rerunning does not.
	///
	/// The key covers url and body but not headers, and a Places field mask is a header that changes
	/// the response shape. Two callers asking the same body under different masks would share one
	/// entry; today they do not, because they differ in the body as well.
	pub fn cached_post(&self, url: &str, body: &serde_json::Value, headers: &[(&str, &str)]) -> Result<(serde_json::Value, SystemTime)> {
		if !self.refresh
			&& let Some(hit) = self.cached(url, body)?
		{
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
			if url == crate::poi::SEARCH_TEXT {
				self.exhausted(&text)?;
			}
			bail!("POST {url} -> {status}\n{}", text.trim());
		}
		self.billed.set(self.billed.get() + 1);
		// the search-volume providers come through here too, and they are other people's quotas
		if url == crate::poi::SEARCH_TEXT {
			self.spend()?;
		}
		let json: serde_json::Value = serde_json::from_str(&text).wrap_err_with(|| format!("POST {url} returned non-JSON: {}", &text[..text.len().min(400)]))?;
		fs::write(&dst, &text)?;
		Ok((json, SystemTime::now()))
	}

	/// The response to this exact request and when it was written, if it has been made before.
	/// Reading the probe's orderings back this way is what keeps a refit from turning into a
	/// purchase.
	pub fn cached(&self, url: &str, body: &serde_json::Value) -> Result<Option<(serde_json::Value, SystemTime)>> {
		let dst = self.post_path(url, &serde_json::to_string(body)?)?;
		let Ok(s) = fs::read_to_string(&dst) else { return Ok(None) };
		let json = serde_json::from_str(&s).wrap_err_with(|| format!("cached response {} is not JSON", dst.display()))?;
		Ok(Some((json, mtime(&dst)?)))
	}

	fn post_path(&self, url: &str, canonical: &str) -> Result<PathBuf> {
		let key = hex(Sha256::digest(format!("{url}\n{canonical}").as_bytes()).as_slice());
		Ok(self.dir("data/places_cache")?.join(format!("{key}.json")))
	}

	/// A cached GET, and when the answer was written.
	pub fn cached_get(&self, url: &str, dst: &str) -> Result<(serde_json::Value, SystemTime)> {
		let dst = self.dir("data")?.join(dst);
		if let Ok(s) = fs::read_to_string(&dst) {
			let json = serde_json::from_str(&s).wrap_err_with(|| format!("cached response {} is not JSON", dst.display()))?;
			return Ok((json, mtime(&dst)?));
		}
		let text = ureq::get(url).call().wrap_err_with(|| format!("GET {url}"))?.into_body().read_to_string()?;
		let json = serde_json::from_str(&text).wrap_err_with(|| format!("GET {url} returned non-JSON: {}", &text[..text.len().min(400)]))?;
		fs::write(&dst, &text)?;
		Ok((json, SystemTime::now()))
	}

	fn ledger_path(&self) -> Result<PathBuf> {
		Ok(self.dir("")?.join("places_budget.json"))
	}

	/// What the window holds, rolled forward to today. Read per call rather than held: two runs of
	/// this binary share one project's quota.
	fn ledger(&self) -> Result<Ledger> {
		let now = now_s();
		let path = self.ledger_path()?;
		let mut l = match fs::read(&path) {
			Ok(b) => serde_json::from_slice(&b).wrap_err_with(|| format!("{} is not a quota ledger", path.display()))?,
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ledger {
				window_start: now - (now - WINDOW_PHASE) % DAY,
				spent: 0,
			},
			Err(e) => return Err(e).wrap_err_with(|| format!("reading {}", path.display())),
		};
		// whole days, so the boundary keeps the phase a 429 taught it
		let elapsed = now.saturating_sub(l.window_start);
		if elapsed >= DAY {
			l.window_start += elapsed / DAY * DAY;
			l.spent = 0;
		}
		Ok(l)
	}

	fn write_ledger(&self, l: &Ledger) -> Result<()> {
		let path = self.ledger_path()?;
		fs::write(&path, serde_json::to_string(l)?).wrap_err_with(|| format!("writing {}", path.display()))
	}

	fn spend(&self) -> Result<()> {
		let mut l = self.ledger()?;
		l.spent += 1;
		self.write_ledger(&l)
	}

	/// A refusal that names the daily search quota says what this ledger was guessing at: when the
	/// window started, and how big it is. Take the first and report the second — the limit is the
	/// config's to state.
	fn exhausted(&self, body: &str) -> Result<()> {
		let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { return Ok(()) };
		let Some(m) = v["error"]["details"]
			.as_array()
			.into_iter()
			.flatten()
			.map(|d| &d["metadata"])
			.find(|m| m["quota_limit"].as_str() == Some(QUOTA))
		else {
			return Ok(());
		};
		let mut l = self.ledger()?;
		if let Some(start) = m["window_start_time"].as_str().and_then(|s| s.parse::<u64>().ok()) {
			l.window_start = start;
		}
		l.spent = self.per_day;
		self.write_ledger(&l)?;
		if let Some(limit) = m["quota_limit_value"].as_str().and_then(|s| s.parse::<u32>().ok())
			&& limit != self.per_day
		{
			eprintln!("{QUOTA} is {limit}, not the {} this is configured for — set `places.per_day` to {limit}", self.per_day);
		}
		Ok(())
	}
}

/// `{window_start, spent}` under the work dir. `SearchTextRequestPerDayPerProject` is not readable
/// with an API key — `serviceusage` answers `API_KEY_SERVICE_BLOCKED` and `monitoring` refuses API
/// keys outright — and both want an OAuth2 principal for a number that `cached_post`, the one place
/// a billed request leaves from, can count locally.
#[derive(Debug, Deserialize, Serialize)]
struct Ledger {
	/// Unix seconds.
	window_start: u64,
	spent: u32,
}

fn now_s() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).expect("the clock is after 1970").as_secs()
}

fn mtime(p: &Path) -> Result<SystemTime> {
	fs::metadata(p)?.modified().wrap_err_with(|| format!("{} has no mtime", p.display()))
}

fn days(d: Duration) -> f64 {
	d.as_secs_f64() / 86_400.
}

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The body a spent window actually returns, trimmed to the detail the ledger reads.
	const REFUSAL: &str = r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED","details":[
		{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"RATE_LIMIT_EXCEEDED","metadata":{
			"window_start_time":"1789369200","quota_limit_value":"100",
			"quota_metric":"places.googleapis.com/SearchTextRequest",
			"quota_limit":"SearchTextRequestPerDayPerProject"}}]}}"#;

	#[test]
	fn a_refusal_closes_the_window_it_names() {
		let tmp = std::env::temp_dir().join("gop_work_refusal");
		let _ = fs::remove_dir_all(&tmp);
		let work = Work::at(&tmp);

		work.exhausted(REFUSAL).unwrap();
		// rolled forward by whole days, so the boundary a 429 taught it survives
		assert_eq!(work.ledger().unwrap().window_start % DAY, WINDOW_PHASE);

		work.write_ledger(&Ledger { window_start: now_s(), spent: 98 }).unwrap();
		work.preflight("a study", Need::Exact(2)).unwrap();
		let refused = work.preflight("a study", Need::AtLeast(3)).unwrap_err().to_string();
		assert!(refused.contains("at least 3"), "{refused}");
		assert!(refused.contains("2 of today's 100"), "{refused}");

		fs::remove_dir_all(&tmp).unwrap();
	}
}
