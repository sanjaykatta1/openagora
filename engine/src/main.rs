use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use openagora::catalog::{Catalog, load_listing};
use openagora::manifest::{Arch, Os};

#[derive(Parser)]
#[command(
    name = "openagora",
    version,
    about = "Install and run open-source apps from the OpenAgora catalog"
)]
struct Cli {
    /// Read listings from this folder (contains apps/<id>/app.toml) instead of
    /// the catalog built into this binary.
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
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

fn load_catalog(dir: &Option<PathBuf>) -> anyhow::Result<Catalog> {
    match dir {
        Some(dir) => Catalog::load(dir),
        None => Catalog::embedded(),
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Cmd::Catalog { query } => {
            let catalog = load_catalog(&cli.catalog)?;
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
            let catalog = load_catalog(&cli.catalog)?;
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
                        if let Some(d) = &step.download {
                            let asset = Arch::current().and_then(|a| d.asset.for_arch(a));
                            println!(
                                "  download {} from github.com/{} (latest release)",
                                asset.unwrap_or("(no build for this CPU)"),
                                d.github
                            );
                        }
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
                let catalog = load_catalog(&cli.catalog)?;
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
