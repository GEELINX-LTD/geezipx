//! GeeZipX CLI — high-performance compression/decompression tool.
//!
//! Phase 1 MVP: `compress`, `decompress`, `list` subcommands.
//!
//! This binary is a thin shell over `geezipx-core`. All archive and
//! compression logic lives in the core crate.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(
    name = "geezipx",
    version,
    about = "High-performance compression and decompression tool"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create an archive or compressed file from one or more inputs
    #[command(visible_alias = "c")]
    Compress {
        /// Input files or directories
        #[arg(required = true)]
        inputs: Vec<PathBuf>,

        /// Output file path
        #[arg(short = 'o', long = "output", required = true)]
        output: PathBuf,

        /// Archive format: zip, tar, tar.gz, tgz, gz, gzip (default: derived from output extension or zip)
        #[arg(short = 'f', long = "format")]
        format: Option<String>,

        /// Recursively add directories
        #[arg(short = 'r', long = "recursive")]
        recursive: bool,
    },

    /// Decompress an archive or compressed file
    #[command(visible_alias = "d", visible_alias = "x")]
    Decompress {
        /// Archive file to decompress
        archive: PathBuf,

        /// Output directory (default: current directory)
        #[arg(short = 'o', long = "output-dir", default_value = ".")]
        output_dir: PathBuf,

        /// Decompress to stdout (gzip only; error for multi-file archives)
        #[arg(long = "stdout")]
        stdout: bool,

        /// Skip files that already exist (mutually exclusive with --force)
        #[arg(long = "no-clobber", conflicts_with = "force")]
        no_clobber: bool,

        /// Overwrite existing files (default; mutually exclusive with --no-clobber)
        #[arg(long = "force", conflicts_with = "no_clobber")]
        force: bool,
    },

    /// List the contents of an archive
    #[command(visible_alias = "l")]
    List {
        /// Archive file
        archive: PathBuf,

        /// Output as JSON array
        #[arg(short = 'j', long = "json")]
        json: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compress {
            inputs,
            output,
            format,
            recursive,
        } => commands::compress::execute(&inputs, &output, format.as_deref(), recursive)?,
        Commands::Decompress {
            archive,
            output_dir,
            stdout,
            no_clobber,
            force: _, // force is explicit default; no-clobber controls behavior
        } => commands::decompress::execute(&archive, &output_dir, stdout, !no_clobber)?,
        Commands::List { archive, json } => commands::list::execute(&archive, json)?,
    }

    Ok(())
}
