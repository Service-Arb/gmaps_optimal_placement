//! The server half. It holds a set of trades and a set of locations and serves the map over their
//! product.
//!
//! `LeptosOptions` is built here rather than read from `Cargo.toml`: this ships as a CLI that runs
//! from wherever the studies are, and `--port` has to win over a manifest it may never see.
//!
//! Building a study is the CLI's job, so it arrives as a closure: the web crate never learns what a
//! `.nix` file is, nor that a trade is a function of a location.
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{Router, extract::Path as UrlPath, http::StatusCode, routing::get};
use eyre::{Result, WrapErr};
use leptos::prelude::*;

/// A trade and a location, to the serialised `Payload` of pairing them.
pub type Build = Arc<dyn Fn(&std::path::Path, &std::path::Path) -> Result<String> + Send + Sync>;

/// One cell per pairing, so two tabs asking at once read the 87 MB archive once between them. Keyed
/// by the two stems rather than by the study's name: what a pairing is called is the CLI's to say,
/// and a second spelling of it here is a second thing to keep in step with the pin file.
type Pool = Arc<HashMap<(String, String), (PathBuf, PathBuf, Arc<tokio::sync::OnceCell<axum::body::Bytes>>)>>;

/// `prebuilt` is `(trade stem, location stem, payload)` — whatever the CLI already paid for.
pub fn serve(trades: Vec<PathBuf>, locations: Vec<PathBuf>, prebuilt: Vec<(String, String, String)>, build: Build, addr: SocketAddr, open: bool) -> Result<()> {
	// leptos spawns the SSR stream through `any_spawner`, which has no executor until it is told
	any_spawner::Executor::init_tokio().map_err(|e| eyre::eyre!("{e}"))?;
	let key = std::env::var("GOOGLE_MAPS_KEY").wrap_err("GOOGLE_MAPS_KEY is not set")?;
	let root = site_root();
	let pkg = root.join("pkg");
	eyre::ensure!(
		pkg.join("gmaps_optimal_placement_web.js").exists(),
		"no client build under {} — run `cargo leptos build` or nix's own",
		pkg.display()
	);

	let options = LeptosOptions::builder()
		.output_name("gmaps_optimal_placement_web")
		.site_root(root.to_string_lossy().into_owned())
		.site_addr(addr)
		.build();

	eyre::ensure!(
		!trades.is_empty() && !locations.is_empty(),
		"the product of {} trades and {} locations is empty",
		trades.len(),
		locations.len()
	);
	let stems = |v: &[PathBuf]| -> Vec<String> { v.iter().map(|p| p.file_stem().expect("a *.nix path has a stem").to_string_lossy().into_owned()).collect() };
	let (trade_stems, location_stems) = (stems(&trades), stems(&locations));

	let mut pool: HashMap<_, _> = trades
		.iter()
		.zip(&trade_stems)
		.flat_map(|(t, ts)| {
			locations
				.iter()
				.zip(&location_stems)
				.map(move |(l, ls)| ((ts.clone(), ls.clone()), (t.clone(), l.clone(), Arc::new(tokio::sync::OnceCell::new()))))
		})
		.collect();
	let opened: Vec<[String; 2]> = prebuilt.iter().map(|(t, l, _)| [t.clone(), l.clone()]).collect();
	for (t, l, json) in prebuilt {
		let (_, _, cell) = pool
			.get_mut(&(t.clone(), l.clone()))
			.ok_or_else(|| eyre::eyre!("{t:?} x {l:?} was built but is not in the product"))?;
		cell.set(json.into()).expect("each study is prebuilt at most once");
	}
	let pool: Pool = Arc::new(pool);

	let title = match (trade_stems.as_slice(), location_stems.as_slice()) {
		([t], [l]) => format!("{t} in {l}"),
		(t, l) => format!("{} trades over {} locations", t.len(), l.len()),
	};
	let shell = {
		let (options, title, key) = (options.clone(), title.clone(), key);
		move || crate::shell(options.clone(), title.clone(), key.clone())
	};

	let studies = serde_json::json!({ "trades": trade_stems, "locations": location_stems, "open": opened }).to_string();
	let mut app = Router::new()
		.route("/studies.json", get(move || std::future::ready(json(studies.clone()))))
		.route(
			"/payload/{trade}/{location}",
			get(move |UrlPath(at): UrlPath<(String, String)>| payload(pool.clone(), build.clone(), at)),
		)
		.nest_service("/pkg", tower_http::services::ServeDir::new(pkg));
	for (path, method) in leptos::server_fn::axum::server_fn_paths() {
		app = app.route(
			path,
			axum::routing::MethodRouter::new().on(
				axum::routing::MethodFilter::try_from(method).expect("leptos only registers routable methods"),
				leptos_axum::handle_server_fns,
			),
		);
	}
	let app = app
		.fallback(get(leptos_axum::render_app_to_stream(shell)))
		.layer(tower_http::compression::CompressionLayer::new())
		.with_state(options);

	let rt = tokio::runtime::Runtime::new().wrap_err("starting the server runtime")?;
	rt.block_on(async move {
		let listener = tokio::net::TcpListener::bind(&addr).await.wrap_err_with(|| format!("binding {addr}"))?;
		let url = format!("http://{}", listener.local_addr()?);
		eprintln!("{title} is at {url}");
		if open {
			// not being able to reach a browser is not a reason to stop serving
			if let Err(e) = std::process::Command::new("xdg-open").arg(&url).spawn() {
				eprintln!("xdg-open: {e}");
			}
		}
		axum::serve(listener, app.into_make_service()).await.wrap_err("serving")
	})
}

/// Built on the first tab that asks for it. A failure is the eyre chain, which the page banners.
async fn payload(pool: Pool, build: Build, at: (String, String)) -> Result<([(axum::http::HeaderName, &'static str); 1], axum::body::Bytes), (StatusCode, String)> {
	let Some((trade, location, cell)) = pool.get(&at) else {
		return Err((StatusCode::NOT_FOUND, format!("{:?} is not a trade over {:?}", at.0, at.1)));
	};
	let built = cell
		.get_or_try_init(|| {
			let (trade, location, build) = (trade.clone(), location.clone(), build.clone());
			async move {
				let shown = format!("{} x {}", trade.display(), location.display());
				tokio::task::spawn_blocking(move || build(&trade, &location).map(axum::body::Bytes::from))
					.await
					.map_err(|e| format!("building {shown}: {e}"))?
					.map_err(|e| format!("{e:?}"))
			}
		})
		.await
		.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
	Ok(json(built.clone()))
}

fn json<T>(body: T) -> ([(axum::http::HeaderName, &'static str); 1], T) {
	([(axum::http::header::CONTENT_TYPE, "application/json")], body)
}

/// Where `wasm-bindgen` put the client. The nix wrapper sets it; a dev shell gets `cargo`'s own.
fn site_root() -> PathBuf {
	std::env::var_os("LEPTOS_SITE_ROOT").map_or_else(|| PathBuf::from("target/site"), PathBuf::from)
}
