//! Ponto de entrada da CLI: lê os argumentos, roda a busca e imprime o relatório.

use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::{Parser, ValueEnum};
use dupfind::{ScanOptions, find_duplicates, report, size::parse_size};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

#[derive(Debug, Parser)]
#[command(
    name = "dupfind",
    version,
    about = "Find duplicate files fast: size pre-filter, partial hash, then parallel BLAKE3."
)]
struct Cli {
    #[arg(default_value = ".", help = "Directories or files to scan")]
    paths: Vec<PathBuf>,

    #[arg(
        short = 's',
        long,
        default_value = "1",
        value_parser = parse_size,
        value_name = "SIZE",
        help = "Ignore files smaller than SIZE (e.g. 512, 10K, 1.5M; binary units)"
    )]
    min_size: u64,

    #[arg(
        short = 'H',
        long,
        help = "Skip hidden files and directories (dot-files; also the hidden attribute on Windows)"
    )]
    ignore_hidden: bool,

    #[arg(short, long, value_enum, default_value_t = Format::Text, help = "Output format")]
    format: Format,

    #[arg(
        short = 'j',
        long,
        value_parser = clap::value_parser!(u16).range(1..),
        help = "Hashing threads [default: number of logical CPUs]"
    )]
    threads: Option<u16>,

    #[arg(short, long, help = "Do not print warnings about unreadable files")]
    quiet: bool,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        // `dupfind | head` fecha o pipe antes do fim: não é falha do programa.
        Err(err) if is_broken_pipe(&err) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("dupfind: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    // Caminho digitado errado é erro de uso; já arquivo ilegível no meio da
    // varredura só vira aviso.
    for path in &cli.paths {
        let exists = path
            .try_exists()
            .with_context(|| format!("cannot access {}", path.display()))?;
        if !exists {
            bail!("path not found: {}", path.display());
        }
    }

    if let Some(threads) = cli.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(usize::from(threads))
            .build_global()
            .context("failed to start the thread pool")?;
    }

    let options = ScanOptions {
        min_size: cli.min_size,
        ignore_hidden: cli.ignore_hidden,
    };
    let result = find_duplicates(&cli.paths, &options);

    if !cli.quiet {
        for error in &result.errors {
            eprintln!("warning: {error}");
        }
    }

    let mut out = BufWriter::new(io::stdout().lock());
    match cli.format {
        Format::Text => report::write_text(&mut out, &result)?,
        Format::Json => report::write_json(&mut out, &result)?,
    }
    out.flush()?;
    Ok(())
}

fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.downcast_ref::<io::Error>()
        .is_some_and(|io| io.kind() == io::ErrorKind::BrokenPipe)
}
