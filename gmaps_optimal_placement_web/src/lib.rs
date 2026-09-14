#![feature(default_field_values)]
#![recursion_limit = "256"] // nested tachys view types blow the default 128 on the control panel
#![doc = include_str!("../README.md")]

pub mod map;
pub mod pins;
#[cfg(feature = "ssr")]
pub mod serve;
pub mod tabs;

use leptos::prelude::*;

const CSS: &str = include_str!("map.css");

#[component]
pub fn App() -> impl IntoView {
	view! { <map::MapView /> }
}

/// `map_core.js` awaits `__mapsReady` before it touches `google.maps`, so the callback the Maps
/// bootstrap fires is a promise resolver rather than an entry point — nothing races the wasm.
pub fn shell(options: LeptosOptions, study: String, key: String) -> impl IntoView {
	let maps = format!("https://maps.googleapis.com/maps/api/js?key={key}&callback=__gmapsCb&loading=async");
	view! {
		<!DOCTYPE html>
		<html lang="en">
			<head>
				<meta charset="utf-8" />
				<meta name="viewport" content="width=device-width, initial-scale=1" />
				<title>{study}</title>
				{leptos::html::style().inner_html(CSS)}
				{leptos::html::script()
					.inner_html("window.__mapsReady = new Promise(r => { window.__gmapsCb = r; });")}
				{leptos::html::script().attr("async", "").attr("src", maps)}
				<AutoReload options=options.clone() />
				<HydrationScripts options islands=true />
			</head>
			<body>
				<App />
			</body>
		</html>
	}
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
	console_error_panic_hook::set_once();
	leptos::mount::hydrate_islands();
}
