//! The map is one file that opens from disk. The template is embedded, so the binary is too.
use eyre::{Result, WrapErr, ensure};

use crate::{Payload, SearchPayload};

const TEMPLATE: &str = include_str!("map_template.html");
const SEARCHES: &str = include_str!("searches_template.html");

pub fn render(payload: &Payload) -> Result<String> {
	let key = std::env::var("GOOGLE_MAPS_KEY").wrap_err("GOOGLE_MAPS_KEY is not set")?;
	let data = serde_json::to_string(payload)?;
	fill(TEMPLATE, &[("/*__DATA__*/null", &data), ("__TITLE__", &payload.name), ("__KEY__", &key)])
}

pub fn render_searches(payload: &SearchPayload) -> Result<String> {
	let data = serde_json::to_string(payload)?;
	fill(SEARCHES, &[("/*__DATA__*/null", &data), ("__TITLE__", &payload.name)])
}

fn fill(template: &str, subs: &[(&str, &str)]) -> Result<String> {
	let mut html = template.to_owned();
	for (tag, value) in subs {
		ensure!(html.contains(tag), "placeholder {tag} missing from the template");
		html = html.replacen(tag, value, 1);
	}
	Ok(html)
}
