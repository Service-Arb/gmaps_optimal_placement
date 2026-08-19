use std::path::PathBuf;

use clap::Parser;
use eyre::Result;
use service_arb_sources::Work;

#[derive(Parser)]
#[command(about = "Paint a study's demand model over the competitors already on the ground")]
struct Cli {
	/// The study document
	config: Option<PathBuf>,
	/// Where the map goes; defaults to `<work dir>/out/<study name>.html`
	#[arg(short, long)]
	out: Option<PathBuf>,
	/// Print the config JSON schema and exit
	#[arg(long)]
	schema: bool,
}

fn main() -> Result<()> {
	color_eyre::install()?;
	let cli = Cli::parse();
	if cli.schema {
		println!("{}", serde_json::to_string_pretty(&schemars::schema_for!(service_arb::Study))?);
		return Ok(());
	}
	let Some(config) = cli.config else {
		<Cli as clap::CommandFactory>::command().print_help()?;
		return Ok(());
	};

	let study = service_arb::load(&config)?;
	let work = Work::from_env();
	let payload = study.build(&work)?;
	for (k, v) in service_arb::stats(&payload) {
		eprintln!("  {k}: {v}");
	}

	let out = cli.out.unwrap_or_else(|| work.path().join("out").join(format!("{}.html", payload.name)));
	if let Some(dir) = out.parent() {
		std::fs::create_dir_all(dir)?;
	}
	let html = service_arb::render::render(&payload)?;
	std::fs::write(&out, &html)?;
	eprintln!("wrote {} ({:.1} MB)", out.display(), html.len() as f64 / 1e6);
	Ok(())
}
