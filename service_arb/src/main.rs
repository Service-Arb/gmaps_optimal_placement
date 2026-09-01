use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};
use eyre::Result;
use service_arb_sources::Work;

#[derive(Parser)]
#[command(about = "Paint a study's demand model over the competitors already on the ground")]
struct Cli {
	#[command(subcommand)]
	cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
	/// Where the people are: demand under the competitors, as a map served from here
	Serve {
		config: PathBuf,
		#[arg(short, long, default_value_t = 8731)]
		port: u16,
		/// Point the desktop browser at it once it is up
		#[arg(long)]
		open: bool,
	},
	/// How many people ask for it: monthly volume per query group, as one HTML chart
	Searches {
		config: PathBuf,
		/// Defaults to `<work dir>/out/<study name>-searches.html`
		#[arg(short, long)]
		out: Option<PathBuf>,
	},
	/// Print the study document's JSON schema
	Schema,
}

fn main() -> Result<()> {
	color_eyre::install()?;
	let work = Work::from_env();
	match Cli::parse().cmd {
		Cmd::Schema => {
			println!("{}", serde_json::to_string_pretty(&schemars::schema_for!(service_arb::Study))?);
			Ok(())
		}
		Cmd::Serve { config, port, open } => {
			let payload = service_arb::load(&config)?.build(&work)?;
			report(service_arb::stats(&payload));
			service_arb_web::serve::serve(payload, SocketAddr::from(([127, 0, 0, 1], port)), open)
		}
		Cmd::Searches { config, out } => {
			let payload = service_arb::load(&config)?.searches(&work)?;
			report(service_arb::search_stats(&payload));
			let html = service_arb::render::render_searches(&payload)?;
			let out = out.unwrap_or_else(|| work.path().join("out").join(format!("{}-searches.html", payload.name)));
			if let Some(dir) = out.parent() {
				std::fs::create_dir_all(dir)?;
			}
			std::fs::write(&out, &html)?;
			eprintln!("wrote {} ({:.1} MB)", out.display(), html.len() as f64 / 1e6);
			Ok(())
		}
	}
}

fn report(stats: indexmap::IndexMap<String, String>) {
	for (k, v) in stats {
		eprintln!("  {k}: {v}");
	}
}
