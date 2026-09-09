//! The map island. Rust owns every number and the whole control surface; `map_core.js` owns
//! `google.maps` and the canvas loop, because that one needs the live projection every frame.
use std::rc::Rc;

use gmaps_optimal_placement_core::{
	Model,
	model::{Report, TierState, fmt, ramp},
};
use leptos::prelude::*;

use crate::pins::Pin;

/// One per tier, in the study's own order. Sixth and beyond wrap.
const TIER_COLORS: [&str; 5] = ["#ff2d55", "#00d0ff", "#ffb020", "#7ae77a", "#c78bff"];

/// Everything the view reads and the effects write. Copy, so a closure captures it by value.
#[derive(Clone, Copy)]
pub struct State {
	pub heavy: StoredValue<Heavy, LocalStorage>,
	pub lambda: RwSignal<f64>,
	pub opacity: RwSignal<f64>,
	pub layer: RwSignal<usize>,
	pub hide_imputed: RwSignal<bool>,
	pub tiers: RwSignal<Vec<TierState>>,
	/// Bumped by every recompute, so the cheap effects can depend on it without depending on λ.
	pub recomputed: RwSignal<u32>,
	/// Name, note and tier counts, settled once the payload lands.
	pub loaded: RwSignal<Option<Loaded>>,
	pub legend: RwSignal<Option<Legend>>,
	pub report: RwSignal<Option<Report>>,
	/// Which pin's card is open. `None` while a sweep or a comparison is shown.
	pub selected: RwSignal<Option<String>>,
	pub core: RwSignal<Vec<Pin>>,
	pub temp: RwSignal<Vec<Pin>>,
	pub tip: RwSignal<Option<Tip>>,
	pub banner: RwSignal<Option<String>>,
	/// Flips once `map_core` has a map instance: the effects that push arrays into it are no-ops
	/// before that, and this is what gives them their second run.
	pub mounted: RwSignal<bool>,
	pub next_id: StoredValue<u32>,
	/// The `⧉` glyph, flipped for a beat by a copy that landed or one that did not.
	pub copied: RwSignal<Option<bool>>,
}
impl State {
	fn new() -> Self {
		Self {
			heavy: StoredValue::new_local(Heavy::default()),
			lambda: RwSignal::new(2000.),
			opacity: RwSignal::new(0.62),
			layer: RwSignal::new(0),
			hide_imputed: RwSignal::new(false),
			tiers: RwSignal::new(Vec::new()),
			recomputed: RwSignal::new(0),
			loaded: RwSignal::new(None),
			legend: RwSignal::new(None),
			report: RwSignal::new(None),
			selected: RwSignal::new(None),
			core: RwSignal::new(Vec::new()),
			temp: RwSignal::new(Vec::new()),
			tip: RwSignal::new(None),
			banner: RwSignal::new(None),
			mounted: RwSignal::new(false),
			next_id: StoredValue::new(0),
			copied: RwSignal::new(None),
		}
	}

	/// The green set: what the study seeded, minus what was hidden, plus what was promoted. Lettered
	/// in that order.
	pub fn lettered(&self) -> Vec<(String, Pin)> {
		self.core.get().into_iter().enumerate().map(|(i, p)| (letter(i), p)).collect()
	}

	pub fn pin(&self, id: &str) -> Option<Pin> {
		self.core
			.with_untracked(|v| v.iter().find(|p| p.id == id).cloned())
			.or_else(|| self.temp.with_untracked(|v| v.iter().find(|p| p.id == id).cloned()))
	}

	pub fn fresh_id(&self) -> String {
		let n = self.next_id.get_value();
		self.next_id.set_value(n + 1);
		format!("t{n}")
	}

	/// Open a pin's card.
	pub fn open(&self, id: &str) {
		self.copied.set(None);
		self.selected.set(Some(id.to_owned()));
		self.refresh_card();
	}

	/// The selected pin's report, against the controls as they stand. Reads nothing reactively, so
	/// the effect that keeps the card live cannot re-enter itself through `selected`.
	pub fn refresh_card(&self) {
		let Some(pin) = self.selected.get_untracked().and_then(|id| self.pin(&id)) else { return };
		let label = self
			.core
			.get_untracked()
			.iter()
			.position(|p| p.id == pin.id)
			.map(|i| format!("{} · {}", letter(i), pin.name))
			.or_else(|| pin.label.clone());
		self.heavy.with_value(|h| {
			let Some(m) = &h.model else { return };
			self.report.set(Some(m.site_report(
				pin.at,
				label.as_deref(),
				// a ranked pick is a cell centroid with a number for a label, and there is no trading
				// name to score
				pin.label.is_none().then_some(pin.name.as_str()),
				self.lambda.get_untracked(),
				&h.press,
				&self.tiers.get_untracked(),
			)));
		});
	}

	fn drop_temp(&self) {
		self.temp.set(Vec::new());
		self.report.set(None);
		self.selected.set(None);
	}

	fn rank(&self) {
		let picks = self.heavy.with_value(|h| {
			let m = h.model.as_ref()?;
			Some(m.rank_sites(self.lambda.get_untracked(), &h.press, 10))
		});
		let Some((picked, pool)) = picks else { return };
		self.temp.set(
			picked
				.iter()
				.enumerate()
				.map(|(k, r)| Pin {
					id: self.fresh_id(),
					name: String::new(),
					at: [r.lat, r.lng],
					label: Some((k + 1).to_string()),
					green: false,
					from_study: false,
				})
				.collect(),
		);
		self.selected.set(None);
		self.heavy.with_value(|h| {
			let Some(m) = &h.model else { return };
			self.report.set(Some(Report {
				title: format!("Top 10 sites · λ={:.1} km", self.lambda.get_untracked() / 1000.),
				rows: picked
					.iter()
					.enumerate()
					.map(|(k, r)| gmaps_optimal_placement_core::model::Row {
						label: format!("{}. {}", k + 1, m.payload.place[r.cell]),
						sub: None,
						value: fmt(r.score),
					})
					.collect(),
				note: format!("Greedy pick, kept ≥1.5 km apart, from {pool} candidate cells. Click a numbered pin for its full report. These are cell centroids, not addresses."),
			}));
		});
	}

	fn compare(&self) {
		let cands: Vec<gmaps_optimal_placement_core::Candidate> = self
			.core
			.get_untracked()
			.into_iter()
			.map(|p| gmaps_optimal_placement_core::Candidate { name: p.name, at: p.at })
			.collect();
		if cands.is_empty() {
			return;
		}
		self.selected.set(None);
		self.heavy.with_value(|h| {
			let Some(m) = &h.model else { return };
			self.report.set(Some(m.compare(&cands, self.lambda.get_untracked(), &h.press)));
		});
	}
}

#[derive(Default)]
pub struct Heavy {
	pub model: Option<Rc<Model>>,
	pub press: Vec<f64>,
	pub unmet: Vec<f64>,
	pub shown: Vec<u8>,
}

#[derive(Clone, PartialEq)]
pub struct Loaded {
	pub study: String,
	pub layers: Vec<(String, String)>,
	pub tier_counts: Vec<usize>,
	pub imputed: usize,
}

#[derive(Clone, PartialEq)]
pub struct Legend {
	pub ticks: [f64; 5],
	pub count: usize,
	pub linear: bool,
}

#[derive(Clone, PartialEq)]
pub struct Tip {
	pub text: String,
	pub x: f64,
	pub y: f64,
}

#[island]
pub fn MapView() -> impl IntoView {
	let s = State::new();
	imp::wire(s);

	view! {
		<div id="map"></div>
		{move || {
			s.tip
				.get()
				.map(|t| {
					view! {
						<div id="tip" style=format!("display:block;left:{}px;top:{}px", t.x + 14., t.y + 14.)>
							{t.text}
						</div>
					}
				})
		}}
		{move || s.banner.get().map(|b| view! { <div id="banner">{b}</div> })}

		<div class="panel" id="ctl">
			<h4>"Layer"</h4>
			<select on:change=move |ev| {
				s.layer.set(event_target_value(&ev).parse().unwrap_or(0))
			}>
				{move || {
					s.loaded
						.get()
						.map(|l| {
							l.layers
								.into_iter()
								.enumerate()
								.map(|(i, (name, _))| view! { <option value=i.to_string()>{name}</option> })
								.collect_view()
						})
				}}
			</select>

			<label>
				"Catchment radius λ — "
				<span>{move || format!("{:.1} km", s.lambda.get() / 1000.)}</span>
			</label>
			<input
				type="range"
				min="300"
				max="6000"
				step="100"
				prop:value=move || s.lambda.get()
				on:input=move |ev| {
					if let Ok(v) = event_target_value(&ev).parse() {
						s.lambda.set(v);
					}
				}
			/>

			<label>
				"Overlay opacity — " <span>{move || format!("{:.0}%", s.opacity.get() * 100.)}</span>
			</label>
			<input
				type="range"
				min="0"
				max="100"
				step="5"
				prop:value=move || s.opacity.get() * 100.
				on:input=move |ev| {
					if let Ok(v) = event_target_value(&ev).parse::<f64>() {
						s.opacity.set(v / 100.);
					}
				}
			/>

			{move || {
				s.loaded
					.get()
					.map(|l| {
						s.tiers
							.get_untracked()
							.into_iter()
							.enumerate()
							.map(|(i, t)| {
								let (name, count) = (t.name.clone(), l.tier_counts[i]);
								let swatch = format!("background:{}", TIER_COLORS[i % TIER_COLORS.len()]);
								view! {
									<div class="row">
										<input
											type="checkbox"
											id=format!("tier{i}")
											checked=true
											on:change=move |ev| {
												s.tiers.update(|v| v[i].show = event_target_checked(&ev));
											}
										/>
										<label for=format!("tier{i}")>
											<span class="sw" style=swatch></span>
											{format!(" {name} ({count})")}
										</label>
									</div>
									<label>
										{format!("weight of one {} — ", t.name)}
										<span>{move || s.tiers.with(|v| format!("{:.2}", v[i].weight))}</span>
									</label>
									<input
										type="range"
										min="0"
										max="200"
										step="5"
										value=(t.weight * 100.).round().to_string()
										on:input=move |ev| {
											if let Ok(v) = event_target_value(&ev).parse::<f64>() {
												s.tiers.update(|t| t[i].weight = v / 100.);
											}
										}
									/>
								}
							})
							.collect_view()
					})
			}}

			<div class="row">
				<input
					type="checkbox"
					id="hideImp"
					on:change=move |ev| s.hide_imputed.set(event_target_checked(&ev))
				/>
				<label for="hideImp">
					"Hide imputed cells (" {move || s.loaded.get().map(|l| l.imputed.to_string())} ")"
				</label>
			</div>

			<button on:click=move |_| s.rank()>"Rank top 10 sites"</button>
			<button
				style:display=move || if s.core.get().is_empty() { "none" } else { "block" }
				on:click=move |_| s.compare()
			>
				"Compare candidates"
			</button>
			<button class="sec" on:click=move |_| s.drop_temp()>
				"Clear pins & report"
			</button>

			<div id="bar" style=gradient()></div>
			<div id="ticks">
				{move || {
					s.legend.get().map(|l| l.ticks.map(|t| view! { <span>{fmt(t)}</span> }).collect_view())
				}}
			</div>
			<div class="note">
				{move || {
					let (spec, legend) = (s.loaded.get(), s.legend.get());
					let (Some(spec), Some(legend)) = (spec, legend) else { return None };
					let (name, note) = spec.layers.get(s.layer.get()).cloned()?;
					Some(
						view! {
							<b>{plain(&name)}</b>
							<br />
							{note}
							<br />
							<span class="k">
								{format!(
									"{} cells · colour = {}",
									legend.count,
									if legend.linear { "linear" } else { "percentile" },
								)}
							</span>
						},
					)
				}}
			</div>
		</div>

		{move || s.report.get().map(|r| view! { <crate::pins::Card state=s report=r /> })}
	}
}
fn letter(i: usize) -> String {
	char::from_u32('A' as u32 + i as u32).map_or_else(|| (i + 1).to_string(), String::from)
}

/// The `★` is a hint in the dropdown, not part of the layer's name.
fn plain(name: &str) -> String {
	name.replace(" ★", "").trim().to_owned()
}

fn gradient() -> String {
	let stops: Vec<String> = (0..=10)
		.map(|q| {
			let c = ramp(q as f64 / 10.);
			format!("rgb({},{},{}) {}%", c[0].round(), c[1].round(), c[2].round(), q * 10)
		})
		.collect();
	format!("background:linear-gradient(90deg,{})", stops.join(","))
}

#[cfg(not(feature = "hydrate"))]
mod imp {
	/// Server-side the island renders its empty shell; nothing recomputes until the wasm lands.
	pub fn wire(_: super::State) {}
}

#[cfg(feature = "hydrate")]
mod imp {
	use gmaps_optimal_placement_core::model::{LayerSpec, colorise};
	use leptos::prelude::*;
	use wasm_bindgen::{closure::WasmClosure, prelude::*};

	use super::{Legend, State, TIER_COLORS, Tip};
	use crate::pins::Pin;

	/// One per tier, in the study's own order. Sixth and beyond wrap.
	const GREEN: &str = "#7cff8f";
	const YELLOW: &str = "#ffd400";

	/// What a redraw needs, once the controls have moved. Split from the payload so the cheap effects
	/// never pay for a pressure sweep.
	fn recompute(s: State) {
		s.heavy.update_value(|h| {
			let Some(m) = h.model.clone() else { return };
			h.press = m.pressure(s.lambda.get_untracked(), &s.tiers.get_untracked());
			h.unmet = m.unmet(&h.press);
		});
		s.recomputed.update(|n| *n += 1);
	}

	fn colours(s: State) -> Option<(Legend, Vec<u8>, Vec<u8>)> {
		let layers: Vec<LayerSpec> = s.heavy.with_value(|h| h.model.as_ref().map(|m| m.layers()))?;
		let spec = layers.get(s.layer.get_untracked())?;
		let (legend, colors, shown) = s.heavy.with_value(|h| {
			let m = h.model.as_ref().expect("layers only exist once the model does");
			let c = colorise(m.values(spec.source, &h.press, &h.unmet), &m.payload.imputed, s.hide_imputed.get_untracked(), spec.linear);
			(
				Legend {
					ticks: c.ticks,
					count: c.count,
					linear: spec.linear,
				},
				c.colors,
				c.shown,
			)
		});
		s.heavy.update_value(|h| h.shown = shown.clone());
		Some((legend, colors, shown))
	}

	#[wasm_bindgen(module = "/src/map_core.js")]
	extern "C" {
		#[wasm_bindgen(js_name = mount)]
		async fn mount_js(
			el: web_sys::HtmlElement,
			lat: f64,
			lng: f64,
			zoom: u8,
			on_click: &js_sys::Function,
			on_move: &js_sys::Function,
			on_out: &js_sys::Function,
			on_pin: &js_sys::Function,
		) -> JsValue;
		#[wasm_bindgen(js_name = cells)]
		fn cells_js(el: &web_sys::HtmlElement, ring_x: &[f64], ring_y: &[f64], colors: &[u8], shown: &[u8]);
		#[wasm_bindgen(js_name = opacity)]
		fn opacity_js(el: &web_sys::HtmlElement, v: f64);
		#[wasm_bindgen(js_name = competitors)]
		fn competitors_js(el: &web_sys::HtmlElement, json: &str);
		#[wasm_bindgen(js_name = showTiers)]
		fn show_tiers_js(el: &web_sys::HtmlElement, flags: &[u8]);
		#[wasm_bindgen(js_name = pins)]
		fn pins_js(el: &web_sys::HtmlElement, json: &str);
	}

	fn host() -> Option<web_sys::HtmlElement> {
		document().get_element_by_id("map").and_then(|e| e.dyn_into().ok())
	}

	/// Leaked once: the map holds these for the page's whole life, and the island never unmounts.
	fn leak<T: ?Sized + WasmClosure>(c: Closure<T>) -> js_sys::Function {
		c.into_js_value().unchecked_into()
	}

	pub fn wire(s: State) {
		Effect::new(move |ran: Option<()>| {
			if ran.is_some() {
				return;
			}
			leptos::task::spawn_local(async move { load(s).await });
		});

		Effect::new(move |_| {
			s.lambda.track();
			s.tiers.track();
			if s.heavy.with_value(|h| h.model.is_some()) {
				recompute(s);
			}
		});

		Effect::new(move |_| {
			s.recomputed.track();
			s.layer.track();
			s.hide_imputed.track();
			if !s.mounted.get() {
				return;
			}
			let (Some((legend, colors, shown)), Some(el)) = (colours(s), host()) else { return };
			s.legend.set(Some(legend));
			s.heavy.with_value(|h| {
				let m = h.model.as_ref().expect("colours only returns once the model is in");
				cells_js(&el, &m.ring_x, &m.ring_y, &colors, &shown);
			});
		});

		// the open card is a live view of λ, not a snapshot of when it was clicked
		Effect::new(move |_| {
			s.recomputed.track();
			s.selected.track();
			s.refresh_card();
		});

		Effect::new(move |_| {
			let v = s.opacity.get();
			if let Some(el) = host().filter(|_| s.mounted.get()) {
				opacity_js(&el, v);
			}
		});

		Effect::new(move |_| {
			let flags: Vec<u8> = s.tiers.get().iter().map(|t| u8::from(t.show)).collect();
			if let Some(el) = host().filter(|_| s.mounted.get()) {
				show_tiers_js(&el, &flags);
			}
		});

		Effect::new(move |_| {
			let lettered = s.lettered();
			let mut out: Vec<serde_json::Value> = lettered
				.iter()
				.map(|(l, p)| serde_json::json!({"id": p.id, "lat": p.at[0], "lng": p.at[1], "color": GREEN, "label": l, "title": p.name}))
				.collect();
			out.extend(
				s.temp
					.get()
					.iter()
					.map(|p| serde_json::json!({"id": p.id, "lat": p.at[0], "lng": p.at[1], "color": YELLOW, "label": p.label, "title": p.label})),
			);
			if let Some(el) = host().filter(|_| s.mounted.get()) {
				pins_js(&el, &serde_json::to_string(&out).expect("a Vec<Value> serialises"));
			}
		});
	}

	async fn load(s: State) {
		let payload = match gloo_net::http::Request::get("/payload.json").send().await {
			Err(e) => return s.banner.set(Some(format!("⚠ the study never arrived — {e}"))),
			Ok(r) if !r.ok() => return s.banner.set(Some(format!("⚠ the study never arrived — {}", r.status_text()))),
			Ok(r) => match r.json::<gmaps_optimal_placement_core::Payload>().await {
				Ok(p) => p,
				Err(e) => return s.banner.set(Some(format!("⚠ the study did not parse — {e}"))),
			},
		};
		let model = match gmaps_optimal_placement_core::Model::try_new(payload) {
			Ok(m) => std::rc::Rc::new(m),
			Err(e) => return s.banner.set(Some(format!("⚠ {e}"))),
		};

		s.lambda.set(model.payload.lambda_m);
		s.tiers.set(model.tier_states());
		s.core.set(crate::pins::seed(&model.payload).await.unwrap_or_else(|e| {
			s.banner.set(Some(format!("⚠ pins — {e}")));
			Vec::new()
		}));
		s.loaded.set(Some(super::Loaded {
			study: model.payload.name.clone(),
			layers: model.layers().into_iter().map(|l| (l.name, l.note)).collect(),
			tier_counts: model.payload.tiers.iter().map(|t| model.payload.pois.iter().filter(|p| p.poi.tier == t.name).count()).collect(),
			imputed: model.payload.imputed.iter().filter(|&&i| i == 1).count(),
		}));

		let comps: Vec<serde_json::Value> = model
			.payload
			.pois
			.iter()
			.map(|p| {
				let ti = model
					.payload
					.tiers
					.iter()
					.position(|t| t.name == p.poi.tier)
					.expect("the model rejected a payload whose tiers disagree");
				serde_json::json!({
					"lat": p.poi.lat, "lng": p.poi.lng, "name": p.poi.name, "addr": p.poi.addr,
					"kind": if p.poi.kind_label.is_empty() { &p.poi.kind } else { &p.poi.kind_label },
					"tier": p.poi.tier, "rating": p.poi.rating, "n_rev": p.poi.n_rev,
					"web": p.poi.web, "tel": p.poi.tel, "ti": ti, "big": ti == 0,
					"color": TIER_COLORS[ti % TIER_COLORS.len()],
				})
			})
			.collect();

		let (center, zoom) = (model.payload.center, model.payload.zoom);
		s.heavy.update_value(|h| h.model = Some(model));
		recompute(s);

		let Some(el) = host() else {
			return s.banner.set(Some("⚠ the map host element is missing".to_owned()));
		};
		let on_click = leak(Closure::<dyn Fn(f64, f64)>::new(move |lat: f64, lng: f64| {
			let id = s.fresh_id();
			s.temp.update(|v| {
				v.push(Pin {
					id: id.clone(),
					name: String::new(),
					at: [lat, lng],
					label: None,
					green: false,
					from_study: false,
				})
			});
			s.open(&id);
		}));
		let on_move = leak(Closure::<dyn Fn(f64, f64, f64, f64)>::new(move |lat: f64, lng: f64, x: f64, y: f64| {
			let text = s.heavy.with_value(|h| {
				let m = h.model.as_ref()?;
				let i = m.cell_at(lat, lng, &h.shown)?;
				Some(m.tooltip(i, &h.press, &h.unmet))
			});
			s.tip.set(text.map(|text| Tip { text, x, y }));
		}));
		let on_out = leak(Closure::<dyn Fn()>::new(move || s.tip.set(None)));
		let on_pin = leak(Closure::<dyn Fn(String)>::new(move |id: String| s.open(&id)));

		if let Some(msg) = mount_js(el.clone(), center[0], center[1], zoom, &on_click, &on_move, &on_out, &on_pin).await.as_string() {
			return s.banner.set(Some(msg));
		}
		competitors_js(&el, &serde_json::to_string(&comps).expect("a Vec<Value> serialises"));
		s.mounted.set(true);
	}
}
