use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use openagora::catalog::{Catalog, load_listing};
use openagora::manifest::Os;

#[derive(Parser)]
#[command(
    name = "openagora",
    version,
    about = "Install and run open-source apps from the OpenAgora catalog"
)]
struct Cli {
    /// Catalog folder (contains apps/<id>/app.toml).
    #[arg(long, global = true, default_value = "catalog")]
    catalog: PathBuf,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List or search the catalog.
    Catalog {
        /// Only show apps matching this text.
        query: Option<String>,
    },
    /// Show one listing, including the install steps for this OS.
    Show { id: String },
    /// Check listing folders (for catalog contributors and CI).
    Validate {
        /// Listing folders; defaults to the whole catalog.
        dirs: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("openagora: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Cmd::Catalog { query } => {
            let catalog = Catalog::load(&cli.catalog)?;
            let listings = match &query {
                Some(q) => catalog.search(q),
                None => catalog.listings.values().collect(),
            };
            let here = Os::current();
            for l in listings {
                let m = &l.manifest;
                let note = match here {
                    Some(os) if !m.platforms.contains(&os) => "  (not available on this OS)",
                    _ => "",
                };
                println!("{:<10} {:<10} {}{note}", m.id, m.category, m.summary);
            }
        }
        Cmd::Show { id } => {
            let catalog = Catalog::load(&cli.catalog)?;
            let l = catalog
                .listings
                .get(&id)
                .ok_or_else(|| anyhow::anyhow!("no app {id:?} in the catalog"))?;
            let m = &l.manifest;
            println!(
                "{} ({})\n{}\nsource:  {}\nlicense: {}",
                m.name, m.id, m.summary, m.source, m.license
            );
            match Os::current() {
                Some(os) => {
                    println!("install on {os}:");
                    for step in m.steps_for(os) {
                        println!("  {}", step.run);
                    }
                }
                None => println!("install: this OS is not supported"),
            }
            if let Some(agent) = &m.agent {
                println!("agent access (asked at install):");
                for p in &agent.permissions {
                    println!("  [{:?}] {}", p.default, p.description);
                }
            }
        }
        Cmd::Validate { dirs } => {
            if dirs.is_empty() {
                let catalog = Catalog::load(&cli.catalog)?;
                println!("{} listing(s) valid", catalog.listings.len());
            } else {
                for dir in &dirs {
                    load_listing(dir)?;
                    println!("{}: valid", dir.display());
                }
            }
        }
    }
    Ok(())
}
