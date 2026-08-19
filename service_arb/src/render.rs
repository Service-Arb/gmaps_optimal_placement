//! The map is one file that opens from disk. The template is embedded, so the binary is too.
use eyre::{Result, WrapErr, ensure};

use crate::Payload;

const TEMPLATE: &str = include_str!("map_template.html");

pub fn render(payload: &Payload) -> Result<String> {
	let key = std::env::var("GOOGLE_MAPS_KEY").wrap_err("GOOGLE_MAPS_KEY is not set")?;
	let data = serde_json::to_string(payload)?;
	let mut html = TEMPLATE.to_owned();
	for (tag, value) in [("/*__DATA__*/null", data.as_str()), ("__TITLE__", &payload.name), ("__KEY__", &key)] {
		ensure!(html.contains(tag), "placeholder {tag} missing from the template");
		html = html.replacen(tag, value, 1);
	}
	Ok(html)
}
