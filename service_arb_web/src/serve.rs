//! The server half. It holds one study and serves the map over it.
//!
//! `LeptosOptions` is built here rather than read from `Cargo.toml`: this ships as a CLI that runs
//! from wherever the study is, and `--port` has to win over a manifest it may never see.
use std::{net::SocketAddr, path::PathBuf};

use axum::{Router, routing::get};
use eyre::{Result, WrapErr};
use leptos::prelude::*;
use service_arb_core::Payload;

/// Where `wasm-bindgen` put the client. The nix wrapper sets it; a dev shell gets `cargo`'s own.
fn site_root() -> PathBuf {
	std::env::var_os("LEPTOS_SITE_ROOT").map_or_else(|| PathBuf::from("target/site"), PathBuf::from)
}

pub fn serve(payload: Payload, addr: SocketAddr, open: bool) -> Result<()> {
	// leptos spawns the SSR stream through `any_spawner`, which has no executor until it is told
	any_spawner::Executor::init_tokio().map_err(|e| eyre::eyre!("{e}"))?;
	let key = std::env::var("GOOGLE_MAPS_KEY").wrap_err("GOOGLE_MAPS_KEY is not set")?;
	let root = site_root();
	let pkg = root.join("pkg");
	eyre::ensure!(
		pkg.join("service_arb_web.js").exists(),
		"no client build under {} — run `cargo leptos build` or nix's own",
		pkg.display()
	);

	let options = LeptosOptions::builder()
		.output_name("service_arb_web")
		.site_root(root.to_string_lossy().into_owned())
		.site_addr(addr)
		.build();
	// multi-MB, so it is fetched rather than baked into the shell as an island prop
	let json = serde_json::to_string(&payload).wrap_err("serialising the study")?;
	let study = payload.name.clone();

	let shell = {
		let (options, study, key) = (options.clone(), study.clone(), key);
		move || crate::shell(options.clone(), study.clone(), key.clone())
	};
	let mut app = Router::new()
		.route(
			"/payload.json",
			get(move || std::future::ready(([(axum::http::header::CONTENT_TYPE, "application/json")], json.clone()))),
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
		eprintln!("{study} is at {url}");
		if open {
			// not being able to reach a browser is not a reason to stop serving
			if let Err(e) = std::process::Command::new("xdg-open").arg(&url).spawn() {
				eprintln!("xdg-open: {e}");
			}
		}
		axum::serve(listener, app.into_make_service()).await.wrap_err("serving")
	})
}
