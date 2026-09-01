//! Pins and the card they open. Two lifetimes, kept apart: `core` is green, lettered and outlives
//! the session; `temp` is yellow and does not. The one `pins[]` array the old page had mixed them,
//! which is why "clear pins" took the study's own candidates with it.
use leptos::prelude::*;
use service_arb_core::{Payload, model::Report};

use crate::map::State;

#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
	/// A green pin is keyed by its name; a yellow one by a counter. Both are stable for the pin's
	/// life, which is what the card and the marker click agree on.
	pub id: String,
	pub name: String,
	/// [lat, lon]
	pub at: [f64; 2],
	/// Yellow only: the sweep's rank, or nothing for a free click.
	pub label: Option<String>,
	pub green: bool,
	/// Named by the study file, rather than promoted here.
	pub from_study: bool,
}

/// The green set the map opens with.
pub async fn seed(payload: &Payload) -> Result<Vec<Pin>, String> {
	Ok(payload
		.candidates
		.iter()
		.map(|c| Pin {
			id: c.name.clone(),
			name: c.name.clone(),
			at: c.at,
			label: None,
			green: true,
			from_study: true,
		})
		.collect())
}

#[component]
pub fn Card(state: State, report: Report) -> impl IntoView {
	let at = move || state.selected.get().and_then(|id| state.pin(&id)).map(|p| p.at);
	view! {
		<div class="panel" id="report">
			<div class="acts">
				{move || {
					at()
						.map(|at| {
							view! {
								<button
									class="act"
									title="copy a Google Maps link"
									on:click=move |_| copy(state, at)
								>
									{move || match state.copied.get() {
										None => "⧉",
										Some(true) => "✓",
										Some(false) => "✕",
									}}
								</button>
							}
						})
				}}
			</div>
			<h4>{report.title}</h4>
			<table>
				{report
					.rows
					.into_iter()
					.map(|r| {
						view! {
							<tr>
								<td class="k">
									{r.label}
									{r
										.sub
										.map(|s| {
											view! {
												<br />
												<span>"  " {s}</span>
											}
										})}
								</td>
								<td>{r.value}</td>
							</tr>
						}
					})
					.collect_view()}
			</table>
			<div class="note">{report.note}</div>
		</div>
	}
}

#[cfg(not(feature = "hydrate"))]
fn copy(_: State, _: [f64; 2]) {}

/// The coordinates, as the link that opens them where a decision actually gets made.
#[cfg(feature = "hydrate")]
fn copy(state: State, at: [f64; 2]) {
	use wasm_bindgen::prelude::*;

	let url = format!("https://www.google.com/maps/search/?api=1&query={},{}", at[0], at[1]);
	leptos::task::spawn_local(async move {
		let clipboard = window().navigator().clipboard();
		let ok = wasm_bindgen_futures::JsFuture::from(clipboard.write_text(&url)).await.is_ok();
		state.copied.set(Some(ok));
		if ok {
			// a rejection stays on screen: there is nothing in the paste buffer to go looking for
			let reset = Closure::once_into_js(move || state.copied.set(None));
			let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(reset.unchecked_ref(), 1200);
		}
	});
}
