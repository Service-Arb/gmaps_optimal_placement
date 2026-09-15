//! The tab bar, the study picker and the key bindings behind them.
//!
//! A tab is one study's snapshot of the controls — λ, layer, tiers, pins, the open card — and the
//! `Model` those controls read. Switching writes the live signals back into the tab being left and
//! reads the next one into them; there is never a second `google.maps` instance.
//!
//! ```text
//!   GET /studies.json   trades, locations: the two axes the CLI was given
//!                       open:              the pairing it was named, if it was named one
//!        │
//!   ┌────┴──── t ─→ Pool::Study    two fields, what and where; Tab crosses, Enter takes the pair
//!   │  Picker                      ↑ also where a served product starts
//!   └───────── f ─→ Pool::Open     filter the open tabs, switch to one
//! ```
//!
//! One field per axis rather than one list of every pairing: a study is a point on a product, and
//! an agglomeration times a trade list is a long list to read when what you know is one coordinate.
//! Both fields are live at once, so a pairing is two narrowings in either order — which is what a
//! step sequence could not do, having already spent the first choice by the time the second is on
//! screen.
//!
//! This is the only picker: `serve` does not run `fzf` over a directory, because choosing the first
//! study and choosing the fourth should not be two different motions.
//!
//! Chrome consumes `Ctrl+T`, `Ctrl+W` and `Ctrl+1..9` before a page sees them, so the defaults are
//! bare keys. The map affords it: it has no text input outside the picker.
#[cfg(feature = "ssr")]
use std::path::PathBuf;

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::map::State;

/// Which set the picker is filtering, and so how many fields it puts up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pool {
	/// The product served, as one field per axis. Taking it opens the tab.
	Study,
	/// The open tabs; taking one switches to it.
	Open,
}
impl Pool {
	/// The rightmost field, which is the one a click commits from.
	fn last(self) -> usize {
		match self {
			Self::Study => 1,
			Self::Open => 0,
		}
	}

	fn prompt(self, col: usize) -> &'static str {
		match (self, col) {
			(Self::Study, 0) => "what is being sold",
			(Self::Study, _) => "and where",
			(Self::Open, _) => "find an open tab",
		}
	}
}

/// The bare keys, one per action. Digits jump to a tab by position and are not rebindable — there is
/// nothing to rebind them to.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Keymap {
	pub prev: String,
	pub next: String,
	pub close: String,
	pub open: String,
	pub find: String,
}
impl Default for Keymap {
	fn default() -> Self {
		Self {
			prev: "[".to_owned(),
			next: "]".to_owned(),
			close: "w".to_owned(),
			open: "t".to_owned(),
			find: "f".to_owned(),
		}
	}
}
impl Keymap {
	/// In the order the settings panel lists them.
	fn rows(&self) -> [(&'static str, &str); 5] {
		[
			("previous tab", &self.prev),
			("next tab", &self.next),
			("close the tab", &self.close),
			("open a study", &self.open),
			("find an open tab", &self.find),
		]
	}

	pub fn set(&mut self, label: &str, key: String) {
		let slot = match label {
			"previous tab" => &mut self.prev,
			"next tab" => &mut self.next,
			"close the tab" => &mut self.close,
			"open a study" => &mut self.open,
			"find an open tab" => &mut self.find,
			other => unreachable!("the panel only lists rows(): {other:?}"),
		};
		*slot = key;
	}
}

/// Config, not data: a keyboard layout is how this machine is driven, and it does not sit beside the
/// pins.
#[server(input = leptos::server_fn::codec::Json)]
pub async fn save_keys(keys: Keymap) -> Result<(), ServerFnError> {
	let json = serde_json::to_string_pretty(&keys).map_err(|e| ServerFnError::new(format!("{e}")))?;
	std::fs::write(path()?, json).map_err(|e| ServerFnError::new(format!("writing the key file: {e}")))
}

#[server]
pub async fn load_keys() -> Result<Option<Keymap>, ServerFnError> {
	match std::fs::read_to_string(path()?) {
		Ok(s) => Ok(Some(serde_json::from_str(&s).map_err(|e| ServerFnError::new(format!("parsing the key file: {e}")))?)),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
		Err(e) => Err(ServerFnError::new(format!("reading the key file: {e}"))),
	}
}

#[cfg(feature = "ssr")]
fn path() -> Result<PathBuf, ServerFnError> {
	xdg::BaseDirectories::with_prefix("gmaps_optimal_placement")
		.place_config_file("keys.json")
		.map_err(|e| ServerFnError::new(format!("placing the key file: {e}")))
}

#[component]
pub fn TabBar(state: State) -> impl IntoView {
	view! {
		<div id="tabs">
			{move || {
				state
					.tabs
					.get()
					.into_iter()
					.enumerate()
					.map(|(i, t)| {
						view! {
							<span
								class="tab"
								class:on=move || state.active.get() == i
								on:click=move |_| crate::map::activate(state, i)
							>
								{t.label}
								<span
									class="x"
									title="close"
									on:click=move |ev| {
										ev.stop_propagation();
										crate::map::close(state, i);
									}
								>
									"●"
								</span>
							</span>
						}
					})
					.collect_view()
			}}
			<span
				class="tab add"
				title="open a study"
				on:click=move |_| state.picker.set(Some(Pool::Study))
			>
				"+"
			</span>
			<span
				id="gear"
				title="key bindings"
				on:click=move |_| {
					state.arming.set(None);
					state.settings.update(|v| *v = !*v);
				}
			>
				"⚙"
			</span>
		</div>
	}
}

/// ↑/↓ (and `Ctrl-P`/`Ctrl-N`) clamp at both ends rather than wrap, `Tab` crosses to the other
/// field, `Enter` takes what both fields have highlighted, `Escape` closes. A cursor sits on its
/// field's first hit, as `fzf`'s does, and every edit of that query puts it back there — so a
/// pairing is only ever one field away from being the one on screen.
///
/// A click lands its own field's cursor; from the rightmost field it also takes the pair, because
/// there is nothing further left to narrow.
#[component]
pub fn Picker(state: State, pool: Pool) -> impl IntoView {
	let cols = pool.last() + 1;
	// one query and one cursor per axis, and which of them has the caret
	let query = [RwSignal::new(String::new()), RwSignal::new(String::new())];
	let sel = [RwSignal::new(0usize), RwSignal::new(0usize)];
	let side = RwSignal::new(0usize);
	let field = [NodeRef::<leptos::html::Input>::new(), NodeRef::<leptos::html::Input>::new()];

	let hits: [Memo<Vec<(usize, String)>>; 2] = [0, 1].map(|col| {
		Memo::new(move |_| {
			let q = query[col].get();
			let all: Vec<(usize, String)> = match (pool, col) {
				(Pool::Study, 0) => state.trades.get().into_iter().enumerate().collect(),
				(Pool::Study, _) => state.locations.get().into_iter().enumerate().collect(),
				(Pool::Open, _) => state.tabs.get().into_iter().enumerate().map(|(i, t)| (i, t.label)).collect(),
			};
			all.into_iter().filter(|(_, s)| subsequence(&q, s)).collect()
		})
	});
	let at = move |col: usize| hits[col].with(|h| h.get(sel[col].get_untracked()).cloned());
	// a field with no hit has nothing to contribute, and half a pairing opens nothing
	let take = move || match pool {
		Pool::Study => {
			if let (Some((_, trade)), Some((_, location))) = (at(0), at(1)) {
				state.picker.set(None);
				crate::map::open(state, trade, location);
			}
		}
		Pool::Open => {
			if let Some((i, _)) = at(0) {
				state.picker.set(None);
				crate::map::activate(state, i);
			}
		}
	};

	// the overlay is modal, so the keys below are only ever the picker's
	Effect::new(move |_| {
		if let Some(el) = field[side.get()].get() {
			el.focus().expect("the picker's field takes focus");
		}
	});
	// arrowing past the visible rows would otherwise leave the cursor off screen
	Effect::new(move |_| crate::map::scroll_pick(sel[side.get()].get()));

	view! {
		<div id="picker" class="panel" class:two=cols == 2>
			{(0..cols)
				.map(|col| {
					view! {
						<div class="col" class:on=move || side.get() == col>
							<input
								type="text"
								node_ref=field[col]
								placeholder=pool.prompt(col)
								prop:value=move || query[col].get()
								on:input=move |ev| {
									query[col].set(event_target_value(&ev));
									sel[col].set(0);
								}
								on:keydown=move |ev| {
									let last = hits[col].with(|h| h.len()).saturating_sub(1);
									let down = ev.key() == "ArrowDown" || (ev.ctrl_key() && ev.key() == "n");
									let up = ev.key() == "ArrowUp" || (ev.ctrl_key() && ev.key() == "p");
									if down || up {
										ev.prevent_default();
										return sel[col]
											.set(
												if down {
													(sel[col].get_untracked() + 1).min(last)
												} else {
													sel[col].get_untracked().saturating_sub(1)
												},
											);
									}
									if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
										return;
									}
									// the fields are the only tab stops, so this wraps rather than leaving the overlay
									if ev.key() == "Tab" {
										ev.prevent_default();
										return side.set((col + 1) % cols);
									}
									if ev.key() == "Enter" {
										ev.prevent_default();
										take();
									}
								}
							/>
							<ul>
								{move || {
									hits[col]
										.get()
										.into_iter()
										.enumerate()
										.map(|(row, (_, name))| {
											view! {
												<li
													class:on=move || sel[col].get() == row
													on:mouseenter=move |_| sel[col].set(row)
													on:click=move |_| {
														sel[col].set(row);
														side.set(col);
														if col == pool.last() {
															take();
														}
													}
												>
													{name}
												</li>
											}
										})
										.collect_view()
								}}
							</ul>
						</div>
					}
				})
				.collect_view()}
		</div>
	}
}

/// Click a key, then press the new one — the press is read off the document, in `map::key`.
#[component]
pub fn Settings(state: State) -> impl IntoView {
	view! {
		<div id="settings" class="panel">
			<h4>"Keys"</h4>
			{move || {
				state
					.keys
					.get()
					.rows()
					.map(|(label, key)| {
						let key = key.to_owned();
						view! {
							<div class="row">
								<label>{label}</label>
								<button class="sec" on:click=move |_| state.arming.set(Some(label))>
									{move || {
										if state.arming.get() == Some(label) { "…".to_owned() } else { key.clone() }
									}}
								</button>
							</div>
						}
					})
					.collect_view()
			}}
			<div class="note">
				"1–9 jump to a tab, 0 to the last. Click a key, then press the new one."
			</div>
			<button on:click=move |_| {
				state.arming.set(None);
				state.settings.set(false);
			}>"Close"</button>
		</div>
	}
}

/// Every character of `q`, in order, somewhere in `s`. Case-insensitive over ASCII, which is what a
/// file stem is.
fn subsequence(q: &str, s: &str) -> bool {
	let mut hay = s.chars().map(|c| c.to_ascii_lowercase());
	q.chars().all(|c| {
		let c = c.to_ascii_lowercase();
		hay.any(|h| h == c)
	})
}
