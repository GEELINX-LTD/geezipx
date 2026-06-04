//! GeeZipX CLI — high-performance compression/decompression tool.
//!
//! Phase 1 MVP: `compress`, `decompress`, `list` subcommands.
//!
//! This binary is a thin shell over `geezipx-core`. All archive and
//! compression logic lives in the core crate.

use anyhow::Context;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

mod commands;
mod render;
mod signal;
use commands::common;

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

        /// Archive format: zip, tar, tar.gz, tgz, gz, gzip, zst, zstd, tar.zst, tzst, tar.xz, txz, xz, lzma (default: derived from output extension or zip)
        #[arg(short = 'f', long = "format")]
        format: Option<String>,

        /// Recursively add directories
        #[arg(short = 'r', long = "recursive")]
        recursive: bool,

        /// Compression level (0-22, default: varies; gzip/tar.gz/xz/lzma/tar.xz: 0..=9, zstd/tar.zst: 0..=22)
        #[arg(short = 'L', long = "level", value_parser = clap::value_parser!(u32).range(0..=22))]
        level: Option<u32>,

        /// Number of worker threads (0 = auto, default: 1 = single-threaded).
        /// Only zstd and tar.zst currently use multiple threads; other formats
        /// accept this flag for forward compatibility but ignore it.
        #[arg(short = 'j', long = "jobs", default_value_t = 1, value_parser = clap::value_parser!(u32).range(0..=256))]
        jobs: u32,

        /// Encrypt the archive with a password (ZIP AES-256 only).
        /// Using --password with non-ZIP formats will cause an error.
        #[arg(long = "password")]
        password: Option<String>,

        /// Read the encryption password from a file (ZIP AES-256 only).
        /// Mutually exclusive with --password and --password-stdin.
        #[arg(long = "password-file")]
        password_file: Option<PathBuf>,

        /// Read the encryption password from stdin (ZIP AES-256 only).
        /// Mutually exclusive with --password and --password-file.
        #[arg(long = "password-stdin")]
        password_stdin: bool,
    },

    /// Decompress an archive or compressed file
    #[command(visible_alias = "d", visible_alias = "x")]
    Decompress {
        /// Archive file to decompress
        archive: PathBuf,

        /// Output directory (default: current directory)
        #[arg(short = 'o', long = "output-dir", default_value = ".")]
        output_dir: PathBuf,

        /// Decompress to stdout (gzip/zstd/xz/lzma only; error for multi-file/archives)
        #[arg(long = "stdout")]
        stdout: bool,

        /// Skip files that already exist (mutually exclusive with --force)
        #[arg(long = "no-clobber", conflicts_with = "force")]
        no_clobber: bool,

        /// Overwrite existing files (default; mutually exclusive with --no-clobber)
        #[arg(long = "force", conflicts_with = "no_clobber")]
        force: bool,

        /// Password for decrypting encrypted archives (ZIP AES-256).
        #[arg(long = "password")]
        password: Option<String>,

        /// Read the decryption password from a file (ZIP AES-256 only).
        /// Mutually exclusive with --password and --password-stdin.
        #[arg(long = "password-file")]
        password_file: Option<PathBuf>,

        /// Read the decryption password from stdin (ZIP AES-256 only).
        /// Mutually exclusive with --password and --password-file.
        #[arg(long = "password-stdin")]
        password_stdin: bool,
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

    /// Verify the integrity of an archive or compressed file
    #[command(visible_alias = "t")]
    Test {
        /// Archive file to verify
        archive: PathBuf,

        /// Output as JSON
        #[arg(short = 'j', long = "json")]
        json: bool,

        /// Password for decrypting encrypted ZIP archives.
        #[arg(long = "password")]
        password: Option<String>,

        /// Read the verification password from a file (ZIP AES-256 only).
        /// Mutually exclusive with --password and --password-stdin.
        #[arg(long = "password-file")]
        password_file: Option<PathBuf>,

        /// Read the verification password from stdin (ZIP AES-256 only).
        /// Mutually exclusive with --password and --password-file.
        #[arg(long = "password-stdin")]
        password_stdin: bool,
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
            jobs,
            password,
            password_file,
            password_stdin,
        } => {
            let inputs = expand_compress_inputs(&inputs, &output)?;
            let password = common::resolve_password(password, password_file, password_stdin)?;
            commands::compress::execute(
                &inputs,
                &output,
                format.as_deref(),
                recursive,
                level,
                jobs,
                cli.no_progress,
                cli.verbose,
                password,
            )?
        }
        Commands::Decompress {
            archive,
            output_dir,
            stdout,
            no_clobber,
            force: _, // force is explicit default; no-clobber controls behavior
            password,
            password_file,
            password_stdin,
        } => {
            let password = common::resolve_password(password, password_file, password_stdin)?;
            commands::decompress::execute(
                &archive,
                &output_dir,
                stdout,
                !no_clobber,
                cli.no_progress,
                cli.verbose,
                password,
            )?
        }
        Commands::List { archive, json } => commands::list::execute(&archive, json)?,
        Commands::Test {
            archive,
            json,
            password,
            password_file,
            password_stdin,
        } => {
            let password = common::resolve_password(password, password_file, password_stdin)?;
            commands::test::execute(&archive, json, password)?
        }
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            generate(shell, &mut cmd, name, &mut std::io::stdout());
        }
    }

    Ok(())
}

/// Expand glob patterns in compress inputs.
///
/// * Paths without glob metacharacters (`*`, `?`, `[`) are kept as-is.
/// * Paths containing glob metacharacters are expanded via `glob::glob()`.
/// * Duplicates across all inputs are removed (first-occurrence order preserved).
/// * If any glob pattern produces no matches, an error is returned.
/// * If any expanded path equals the output file, an error is returned.
fn expand_compress_inputs(inputs: &[PathBuf], output: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let has_glob_meta = |s: &str| s.contains('*') || s.contains('?') || s.contains('[');

    let mut result = Vec::new();
    let mut seen = HashSet::new();

    for input in inputs {
        let input_str = input.to_string_lossy();
        if has_glob_meta(&input_str) {
            let pattern = input_str.as_ref();
            let entries = glob::glob(pattern)
                .with_context(|| format!("invalid glob pattern '{}'", pattern))?;

            let mut matched = false;
            for entry in entries {
                let path =
                    entry.with_context(|| format!("error reading glob entry for '{}'", pattern))?;
                matched = true;
                if seen.insert(path.clone()) {
                    if path == output {
                        anyhow::bail!(
                            "output file '{}' cannot also be an input (matched by pattern '{}')",
                            output.display(),
                            pattern
                        );
                    }
                    result.push(path);
                }
            }

            if !matched {
                anyhow::bail!("no files matched pattern '{}'", pattern);
            }
        } else {
            if seen.insert(input.clone()) {
                if input == output {
                    anyhow::bail!("output file '{}' cannot also be an input", output.display());
                }
                result.push(input.clone());
            }
        }
    }

    if result.is_empty() {
        anyhow::bail!("at least one input file is required");
    }

    Ok(result)
}
