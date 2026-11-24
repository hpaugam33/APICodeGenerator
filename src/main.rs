//! Command line interface.
//!
//! Usage:
//!     apigen --openapi specs/order-service.openapi.yaml \
//!            --asyncapi specs/order-events.asyncapi.yaml \
//!            --target both --out generated \
//!            --package com.example.orderservice --crate order_service
//!
//! Generation is a FULL REGENERATE: each target directory is wiped and rebuilt,
//! so the specs remain the single source of truth. Keep hand-written code in
//! files/packages the generator does not own.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{ArgAction, Parser};

use apigen::emit::EMITTERS;
use apigen::emit::quarkus::emit_quarkus;
use apigen::emit::rust::emit_rust;
use apigen::loader::load_all;
use apigen::naming::{kebab, snake};

/// Generate service API interfaces + data models from OpenAPI/AsyncAPI specs.
#[derive(Parser)]
#[command(name = "apigen", version, about, long_about = None)]
struct Cli {
    /// OpenAPI 3.x spec file (repeatable)
    #[arg(long, action = ArgAction::Append, value_name = "FILE")]
    openapi: Vec<PathBuf>,

    /// AsyncAPI 2.x/3.x spec file (repeatable)
    #[arg(long, action = ArgAction::Append, value_name = "FILE")]
    asyncapi: Vec<PathBuf>,

    /// which language to generate (default: both)
    #[arg(long, default_value = "both", value_parser = ["both", "quarkus", "rust"])]
    target: String,

    /// output root directory (default: generated)
    #[arg(long, default_value = "generated")]
    out: PathBuf,

    /// Java base package (Quarkus)
    #[arg(long)]
    package: Option<String>,

    /// Maven groupId (Quarkus)
    #[arg(long, default_value = "com.example")]
    group_id: String,

    /// Maven artifactId (Quarkus)
    #[arg(long)]
    artifact: Option<String>,

    /// Rust crate name
    #[arg(long = "crate", value_name = "CRATE")]
    crate_name: Option<String>,

    /// override API name (default: info.title)
    #[arg(long)]
    api_name: Option<String>,
}

fn run(cli: Cli) -> Result<u8> {
    if cli.openapi.is_empty() && cli.asyncapi.is_empty() {
        eprintln!("error: pass at least one --openapi or --asyncapi file");
        return Ok(2);
    }

    let ir = load_all(&cli.openapi, &cli.asyncapi, cli.api_name.as_deref())?;
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let command = format!("apigen {}", argv.join(" "));

    let targets: Vec<&str> = if cli.target == "both" {
        EMITTERS.to_vec()
    } else {
        vec![cli.target.as_str()]
    };
    println!(
        "Loaded {} v{}: {} models, {} REST endpoints, {} message flows",
        ir.api_name,
        ir.version,
        ir.models.len(),
        ir.endpoints.len(),
        ir.flows.len()
    );
    for t in targets {
        let out_dir = cli.out.join(t);
        if out_dir.is_dir() {
            std::fs::remove_dir_all(&out_dir)?; // full regenerate: spec is the source of truth
        }
        std::fs::create_dir_all(&out_dir)?;
        let written = match t {
            "quarkus" => {
                let package = cli.package.clone().unwrap_or_else(|| {
                    format!("com.example.{}", snake(&ir.api_name).replace('_', ""))
                });
                let artifact = cli.artifact.clone().unwrap_or_else(|| kebab(&ir.api_name));
                emit_quarkus(&ir, &out_dir, &package, &cli.group_id, Some(&artifact), &command)?
            }
            _ => {
                let crate_name = cli.crate_name.clone().unwrap_or_else(|| snake(&ir.api_name));
                emit_rust(&ir, &out_dir, Some(&crate_name), &command)?
            }
        };
        println!("  [{t}] wrote {} files -> {}/", written.len(), out_dir.display());
        let mut rels: Vec<String> = written
            .iter()
            .map(|p| {
                p.strip_prefix(&out_dir)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        rels.sort();
        for rel in rels {
            println!("      {rel}");
        }
    }
    Ok(0)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}
