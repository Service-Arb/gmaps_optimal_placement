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

/// The two axes a study is a point on. Either may be a single `*.nix` instead of a directory, which
/// is how a scripted run names one without meeting a picker.
#[derive(Clone, clap::Args)]
struct Pair {
	/// What is being sold: queries, tiering, the demand model, the ranking terms
	#[arg(default_value = "examples/trades")]
	trades: PathBuf,
	/// Where: the frame, the statistics office behind it, the premises in mind
	#[arg(default_value = "examples/locations")]
	locations: PathBuf,
}

#[derive(Subcommand)]
enum Cmd {
	/// Where the people are: demand under the competitors, as a map served from here. The page picks
	/// a trade and a city out of the two directories; naming both as files opens the one tab, already
	/// built
	Serve {
		#[command(flatten)]
		at: Pair,
		#[arg(short, long, default_value_t = 8731)]
		port: u16,
		/// Point the desktop browser at it once it is up
		#[arg(long)]
		open: bool,
	},
	/// How many people ask for it: monthly volume per query group, as one HTML chart
	Searches {
		#[command(flatten)]
		at: Pair,
		/// Defaults to `<work dir>/out/<study name>-searches.html`
		#[arg(short, long)]
		out: Option<PathBuf>,
	},
	/// Ask Google the study's terms from equal-demand nodes, so the ranking model has orderings a
	/// searcher could have produced
	Probe {
		#[command(flatten)]
		at: Pair,
		/// Print the node placement and the call budget, spend nothing
		#[arg(long)]
		dry_run: bool,
	},
	/// Refit the ranking model over every ordering in the work dir, and print the table that goes
	/// into `gmaps_optimal_placement_core::rank::COEF`. How Google ranks is one mechanism, so this takes the
	/// whole product and reads whatever each pairing has already collected
	Fit {
		#[command(flatten)]
		at: Pair,
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
		Cmd::Serve { at, port, open } => {
			// the page picks out of the two directories, so `fzf` is not one of two pickers to learn. A
			// pair named as files is built here instead: its statistics print, and a study that will not
			// build fails before the listener binds
			let (trades, locations) = (gmaps_optimal_placement::files(&at.trades)?, gmaps_optimal_placement::files(&at.locations)?);
			let prebuilt = match (trades.as_slice(), locations.as_slice()) {
				([trade], [location]) => {
					let payload = gmaps_optimal_placement::load(trade, location)?.build(&work)?;
					report(gmaps_optimal_placement::stats(&payload));
					vec![(
						gmaps_optimal_placement::stem(trade).to_owned(),
						gmaps_optimal_placement::stem(location).to_owned(),
						serde_json::to_string(&payload)?,
					)]
				}
				_ => Vec::new(),
			};
			// the work dir rather than `work` itself: `Work` holds a `Cell`, and the server calls this
			// from whichever blocking thread a tab's first request landed on
			let dir = work.path().to_owned();
			let build = std::sync::Arc::new(move |trade: &std::path::Path, location: &std::path::Path| {
				let payload = gmaps_optimal_placement::load(trade, location)?.build(&Work::at(dir.clone()))?;
				Ok(serde_json::to_string(&payload)?)
			});
			gmaps_optimal_placement_web::serve::serve(trades, locations, prebuilt, build, SocketAddr::from(([127, 0, 0, 1], port)), open)
		}
		Cmd::Probe { at, dry_run } => {
			let (trade, location) = (gmaps_optimal_placement::pick(&at.trades)?, gmaps_optimal_placement::pick(&at.locations)?);
			gmaps_optimal_placement::load(&trade, &location)?.probe(&work, dry_run)?;
			Ok(())
		}
		Cmd::Fit { at } => {
			// no picker: how Google ranks is one mechanism, so every pairing that has been harvested
			// contributes. `Study::observations` reads the work dir and never the network, so a pairing
			// nobody ever built costs nothing and says nothing
			let (trades, locations) = (gmaps_optimal_placement::files(&at.trades)?, gmaps_optimal_placement::files(&at.locations)?);
			let studies = trades
				.iter()
				.flat_map(|t| locations.iter().map(move |l| gmaps_optimal_placement::load(t, l)))
				.collect::<Result<Vec<_>>>()?;
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
		Cmd::Searches { at, out } => {
			let (trade, location) = (gmaps_optimal_placement::pick(&at.trades)?, gmaps_optimal_placement::pick(&at.locations)?);
			let payload = gmaps_optimal_placement::load(&trade, &location)?.searches(&work)?;
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
