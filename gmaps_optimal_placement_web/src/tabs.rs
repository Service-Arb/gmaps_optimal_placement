//! The tab bar, the study picker and the key bindings behind them.
//!
//! A tab is one study's snapshot of the controls — λ, layer, tiers, pins, the open card — and the
//! `Model` those controls read. Switching writes the live signals back into the tab being left and
//! reads the next one into them; there is never a second `google.maps` instance.
//!
//! ```text
//!   GET /studies.json   available: every stem in the directory
//!                       open:      the one the CLI was named, if it was named one
//!        │
//!   ┌────┴──── t ─→ Pool::All   filter the directory, open a new tab
//!   │  Picker                   ↑ also where a served directory starts
//!   └───────── f ─→ Pool::Open  filter the open tabs, switch to one
//! ```
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

/// Which set the picker is filtering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pool {
	/// Every study in the directory; picking one opens a tab.
	All,
	/// The open tabs; picking one switches to it.
	Open,
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
				on:click=move |_| state.picker.set(Some(Pool::All))
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

/// ↑/↓ (and `Ctrl-P`/`Ctrl-N`) clamp at both ends rather than wrap, `Enter` takes the highlighted
/// row, `Escape` closes. The cursor sits on the first hit, as `fzf`'s does, and every edit of the
/// query puts it back there.
#[component]
pub fn Picker(state: State, pool: Pool) -> impl IntoView {
	let query = RwSignal::new(String::new());
	let sel = RwSignal::new(0usize);
	let hits = Memo::new(move |_| {
		let q = query.get();
		let all: Vec<(usize, String)> = match pool {
			Pool::All => state.available.get().into_iter().enumerate().collect(),
			Pool::Open => state.tabs.get().into_iter().enumerate().map(|(i, t)| (i, t.label)).collect(),
		};
		all.into_iter().filter(|(_, s)| subsequence(&q, s)).collect::<Vec<_>>()
	});
	let take = move |i: usize, name: String| {
		state.picker.set(None);
		match pool {
			Pool::All => crate::map::open(state, name),
			Pool::Open => crate::map::activate(state, i),
		}
	};
	let field = NodeRef::<leptos::html::Input>::new();
	// the overlay is modal, so the keys below are only ever the picker's
	Effect::new(move |_| {
		if let Some(el) = field.get() {
			el.focus().expect("the picker's field takes focus");
		}
	});
	// arrowing past the visible rows would otherwise leave the cursor off screen
	Effect::new(move |_| crate::map::scroll_pick(sel.get()));

	view! {
		<div id="picker" class="panel">
			<input
				type="text"
				node_ref=field
				placeholder=match pool {
					Pool::All => "open a study",
					Pool::Open => "find an open tab",
				}
				prop:value=move || query.get()
				on:input=move |ev| {
					query.set(event_target_value(&ev));
					sel.set(0);
				}
				on:keydown=move |ev| {
					let last = hits.with(|h| h.len()).saturating_sub(1);
					let down = ev.key() == "ArrowDown" || (ev.ctrl_key() && ev.key() == "n");
					let up = ev.key() == "ArrowUp" || (ev.ctrl_key() && ev.key() == "p");
					if down || up {
						ev.prevent_default();
						return sel
							.set(
								if down {
									(sel.get_untracked() + 1).min(last)
								} else {
									sel.get_untracked().saturating_sub(1)
								},
							);
					}
					if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
						return;
					}
					if ev.key() == "Enter" {
						ev.prevent_default();
						if let Some((i, name)) = hits.with(|h| h.get(sel.get_untracked()).cloned()) {
							take(i, name);
						}
					}
				}
			/>
			<ul>
				{move || {
					hits
						.get()
						.into_iter()
						.enumerate()
						.map(|(row, (i, name))| {
							let pick = name.clone();
							view! {
								<li
									class:on=move || sel.get() == row
									on:mouseenter=move |_| sel.set(row)
									on:click=move |_| take(i, pick.clone())
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
