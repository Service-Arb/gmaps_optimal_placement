//! The search-volume chart is still one file that opens from disk: it has no controls worth
//! persisting and nothing to write back. The map stopped being one — `gmaps_optimal_placement_web` says why.
use eyre::{Result, ensure};

use crate::SearchPayload;

const SEARCHES: &str = include_str!("searches_template.html");

pub fn render_searches(payload: &SearchPayload) -> Result<String> {
	let data = serde_json::to_string(payload)?;
	fill(SEARCHES, &[("/*__DATA__*/null", &data), ("__TITLE__", &payload.name)])
}

fn fill(template: &str, subs: &[(&str, &str)]) -> Result<String> {
	let mut html = template.to_owned();
	for (tag, value) in subs {
		ensure!(html.contains(tag), "placeholder {tag} missing from the template");
		// Every occurrence: the title is in both `<title>` and `<h1>`.
		html = html.replace(tag, value);
	}
	Ok(html)
}
