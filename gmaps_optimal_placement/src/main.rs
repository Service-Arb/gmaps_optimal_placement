use std::{net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};
use eyre::Result;
use gmaps_optimal_placement_sources::Work;

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
	/// Ask Google the study's terms from equal-demand nodes, so the ranking model has orderings a
	/// searcher could have produced
	Probe {
		config: PathBuf,
		/// Print the node placement and the call budget, spend nothing
		#[arg(long)]
		dry_run: bool,
	},
	/// Refit the ranking model over every ordering in the work dir, and print the table that goes
	/// into `gmaps_optimal_placement_core::rank::COEF`. How Google ranks is one mechanism, so pass every study
	/// there is evidence from
	Fit {
		#[arg(required = true)]
		config: Vec<PathBuf>,
	},
	/// Which towns in a country are worth a study: something the cadastre draws, counted per
	/// administrative unit and divided by whatever the statistical grid publishes, as one HTML map
	Misc {
		country: gmaps_optimal_placement::misc::Country,
		tally: gmaps_optimal_placement::misc::Tally,
		/// Grid column to divide by — `ind` reads per person, `men` per household
		#[arg(long, default_value = "ind")]
		per: String,
		/// Skip units under this many `--per`. There is no storefront worth having in a hamlet, and
		/// what is skipped is never downloaded
		#[arg(long, default_value_t = 2000.)]
		floor: f64,
		/// Defaults to `<work dir>/out/<country>-<tally>-per-<column>.html`
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
			println!("{}", serde_json::to_string_pretty(&schemars::schema_for!(gmaps_optimal_placement::Study))?);
			Ok(())
		}
		Cmd::Serve { config, port, open } => {
			let payload = gmaps_optimal_placement::load(&config)?.build(&work)?;
			report(gmaps_optimal_placement::stats(&payload));
			gmaps_optimal_placement_web::serve::serve(payload, SocketAddr::from(([127, 0, 0, 1], port)), open)
		}
		Cmd::Probe { config, dry_run } => {
			gmaps_optimal_placement::load(&config)?.probe(&work, dry_run)?;
			Ok(())
		}
		Cmd::Fit { config } => {
			let studies = config.iter().map(|c| gmaps_optimal_placement::load(c)).collect::<Result<Vec<_>>>()?;
			let lambda: Vec<f64> = studies.iter().map(|s| s.model.lambda_m).collect();
			let fit: gmaps_optimal_placement::fit::Fit = studies.iter().map(|s| s.observations(&work)).collect::<Result<Vec<_>>>()?.into_iter().collect();
			report(fit.stats(&lambda));
			fit.check()
		}
		Cmd::Misc { country, tally, per, floor, out } => {
			let compiled = gmaps_optimal_placement::misc::compile(country, tally, &per, floor, &work)?;
			report(compiled.stats());
			write(out.unwrap_or_else(|| work.path().join("out").join(format!("{}.html", compiled.name()))), &compiled.render()?)
		}
		Cmd::Searches { config, out } => {
			let payload = gmaps_optimal_placement::load(&config)?.searches(&work)?;
			report(gmaps_optimal_placement::search_stats(&payload));
			let html = gmaps_optimal_placement::render::render_searches(&payload)?;
			write(out.unwrap_or_else(|| work.path().join("out").join(format!("{}-searches.html", payload.name))), &html)
		}
	}
}

fn write(out: PathBuf, html: &str) -> Result<()> {
	if let Some(dir) = out.parent() {
		std::fs::create_dir_all(dir)?;
	}
	std::fs::write(&out, html)?;
	eprintln!("wrote {} ({:.1} MB)", out.display(), html.len() as f64 / 1e6);
	Ok(())
}

fn report(stats: indexmap::IndexMap<String, String>) {
	for (k, v) in stats {
		eprintln!("  {k}: {v}");
	}
}
