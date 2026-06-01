//! GeeZipX CLI — high-performance compression/decompression tool.
//!
//! Phase 1 MVP: `compress`, `decompress`, `list` subcommands.
//!
//! This binary is a thin shell over `geezipx-core`. All archive and
//! compression logic lives in the core crate.

use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

mod commands;
mod render;
mod signal;

#[derive(Parser)]
#[command(
    name = "geezipx",
    version,
    about = "High-performance compression and decompression tool"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Disable the progress bar (useful for scripts or non-interactive use)
    #[arg(long = "no-progress", global = true, default_value_t = false)]
    no_progress: bool,

    /// Verbose output: log each file as it's processed
    #[arg(short = 'v', long = "verbose", global = true, default_value_t = false)]
    verbose: bool,
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

        /// Compression level (0-9, default: 6; gzip/tar.gz only)
        #[arg(short = 'L', long = "level", value_parser = clap::value_parser!(u32).range(0..=9))]
        level: Option<u32>,
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

    /// Generate shell completion scripts
    #[command(visible_alias = "comp")]
    Completions {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
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
            level,
        } => commands::compress::execute(
            &inputs,
            &output,
            format.as_deref(),
            recursive,
            level,
            cli.no_progress,
            cli.verbose,
        )?,
        Commands::Decompress {
            archive,
            output_dir,
            stdout,
            no_clobber,
            force: _, // force is explicit default; no-clobber controls behavior
        } => commands::decompress::execute(
            &archive,
            &output_dir,
            stdout,
            !no_clobber,
            cli.no_progress,
            cli.verbose,
        )?,
        Commands::List { archive, json } => commands::list::execute(&archive, json)?,
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            generate(shell, &mut cmd, name, &mut std::io::stdout());
        }
    }

    Ok(())
}
