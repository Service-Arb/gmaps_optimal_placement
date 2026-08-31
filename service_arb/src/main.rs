use std::path::PathBuf;

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
	/// Where the people are: demand under the competitors, as one HTML map
	Map {
		config: PathBuf,
		/// Defaults to `<work dir>/out/<study name>.html`
		#[arg(short, long)]
		out: Option<PathBuf>,
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
	let (out, html, default) = match Cli::parse().cmd {
		Cmd::Schema => {
			println!("{}", serde_json::to_string_pretty(&schemars::schema_for!(service_arb::Study))?);
			return Ok(());
		}
		Cmd::Map { config, out } => {
			let payload = service_arb::load(&config)?.build(&work)?;
			report(service_arb::stats(&payload));
			(out, service_arb::render::render(&payload)?, format!("{}.html", payload.name))
		}
		Cmd::Searches { config, out } => {
			let payload = service_arb::load(&config)?.searches(&work)?;
			report(service_arb::search_stats(&payload));
			(out, service_arb::render::render_searches(&payload)?, format!("{}-searches.html", payload.name))
		}
	};

	let out = out.unwrap_or_else(|| work.path().join("out").join(default));
	if let Some(dir) = out.parent() {
		std::fs::create_dir_all(dir)?;
	}
	std::fs::write(&out, &html)?;
	eprintln!("wrote {} ({:.1} MB)", out.display(), html.len() as f64 / 1e6);
	Ok(())
}

fn report(stats: indexmap::IndexMap<String, String>) {
	for (k, v) in stats {
		eprintln!("  {k}: {v}");
	}
}
