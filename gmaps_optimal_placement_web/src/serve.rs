//! The server half. It holds a directory of studies and serves the map over them.
//!
//! `LeptosOptions` is built here rather than read from `Cargo.toml`: this ships as a CLI that runs
//! from wherever the studies are, and `--port` has to win over a manifest it may never see.
//!
//! Building a study is the CLI's job, so it arrives as a closure: the web crate never learns what a
//! `.nix` file is.
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{Router, extract::Path as UrlPath, http::StatusCode, routing::get};
use eyre::{Result, WrapErr};
use leptos::prelude::*;

/// A study file to its serialised `Payload`.
pub type Build = Arc<dyn Fn(&std::path::Path) -> Result<String> + Send + Sync>;

/// One cell per study, so two tabs asking at once read the 87 MB archive once between them.
type Pool = Arc<HashMap<String, (PathBuf, Arc<tokio::sync::OnceCell<axum::body::Bytes>>)>>;

pub fn serve(dir: PathBuf, prebuilt: Vec<(String, String)>, build: Build, addr: SocketAddr, open: bool) -> Result<()> {
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

	let mut pool: HashMap<_, _> = std::fs::read_dir(&dir)
		.wrap_err_with(|| format!("reading {}", dir.display()))?
		.map(|e| Ok(e?.path()))
		.collect::<std::io::Result<Vec<_>>>()?
		.into_iter()
		.filter(|p| p.extension().is_some_and(|e| e == "nix"))
		.map(|p| {
			let stem = p.file_stem().expect("a *.nix path has a stem").to_string_lossy().into_owned();
			(stem, (p, Arc::new(tokio::sync::OnceCell::new())))
		})
		.collect();
	let open_stems: Vec<String> = prebuilt.iter().map(|(s, _)| s.clone()).collect();
	for (stem, json) in prebuilt {
		let (_, cell) = pool.get_mut(&stem).ok_or_else(|| eyre::eyre!("{stem:?} was built but is not under {}", dir.display()))?;
		cell.set(json.into()).expect("each study is prebuilt at most once");
	}
	let pool: Pool = Arc::new(pool);

	let title = dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned());
	let shell = {
		let (options, title, key) = (options.clone(), title.clone(), key);
		move || crate::shell(options.clone(), title.clone(), key.clone())
	};

	let studies = {
		let mut available: Vec<String> = pool.keys().cloned().collect();
		available.sort();
		serde_json::json!({ "available": available, "open": open_stems }).to_string()
	};
	let mut app = Router::new()
		.route("/studies.json", get(move || std::future::ready(json(studies.clone()))))
		.route("/payload/{stem}", get(move |UrlPath(stem): UrlPath<String>| payload(pool.clone(), build.clone(), stem)))
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
async fn payload(pool: Pool, build: Build, stem: String) -> Result<([(axum::http::HeaderName, &'static str); 1], axum::body::Bytes), (StatusCode, String)> {
	let Some((path, cell)) = pool.get(&stem) else {
		return Err((StatusCode::NOT_FOUND, format!("no study called {stem:?}")));
	};
	let built = cell
		.get_or_try_init(|| {
			let (path, build) = (path.clone(), build.clone());
			async move {
				let shown = path.display().to_string();
				tokio::task::spawn_blocking(move || build(&path).map(axum::body::Bytes::from))
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
