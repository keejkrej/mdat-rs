use std::io::{self, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use mdat::{
    preview_selection, run_convert, run_metadata, OutputFormat, ProgressCallback, ProgressEvent,
    ProgressPhase, Result,
};
use mdat::error::MdatError;
use mdat::input::{inspect_input, open_input};
use mdat_view::ViewArgs;

#[derive(Parser)]
#[command(name = "mdat", about = "Microscopy data utilities for ND2/CZI files.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Export image data to TIFF files under an output directory.
    Convert {
        input_file: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, value_enum, default_value_t = CliOutputFormat::Mdat)]
        format: CliOutputFormat,
        #[arg(long, default_value = "all")]
        position: String,
        #[arg(long, default_value = "all")]
        time: String,
        #[arg(long, default_value = "all")]
        channel: String,
        #[arg(long, default_value = "all")]
        z: String,
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Inspect file metadata without converting image data.
    Metadata {
        input_file: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        raw: bool,
    },
    /// Serve a local viewer for a microscopy dataset.
    View {
        input_file: PathBuf,
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..))]
        idle_timeout: u64,
        #[arg(long)]
        no_open: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum CliOutputFormat {
    Mdat,
    Acdc,
}

impl From<CliOutputFormat> for OutputFormat {
    fn from(value: CliOutputFormat) -> Self {
        match value {
            CliOutputFormat::Mdat => OutputFormat::Mdat,
            CliOutputFormat::Acdc => OutputFormat::Acdc,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Convert {
            input_file,
            output,
            format,
            position,
            time,
            channel,
            z,
            yes,
        } => run_convert_command(
            &input_file,
            &output,
            format.into(),
            &position,
            &time,
            &channel,
            &z,
            yes,
        ),
        Command::Metadata {
            input_file,
            output,
            raw,
        } => run_metadata_command(&input_file, output.as_ref(), raw),
        Command::View {
            input_file,
            idle_timeout,
            no_open,
        } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(mdat_view::run(ViewArgs {
                path: input_file,
                idle_timeout: Some(idle_timeout),
                no_open,
            }))
                .map_err(|e| MdatError::InvalidInput(e.to_string()))
        }
    }
}

fn run_convert_command(
    input_file: &PathBuf,
    output: &PathBuf,
    output_format: OutputFormat,
    position: &str,
    time: &str,
    channel: &str,
    z: &str,
    yes: bool,
) -> Result<()> {
    let input_path = open_input(input_file)?;
    let info = inspect_input(&input_path)?;
    let (summary, selection) = preview_selection(&info, position, time, channel, z)?;

    println!(
        "Input: {} positions, T={}, C={}, Z={}",
        info.n_pos, info.n_time, info.n_chan, info.n_z
    );
    println!("Output format: {}", output_format.as_str());
    println!();
    println!(
        "Selected {}/{} positions, {}/{} timepoints, {}/{} channels, {}/{} z-slices",
        summary.pos_count,
        info.n_pos,
        summary.time_count,
        info.n_time,
        summary.channel_count,
        info.n_chan,
        summary.z_count,
        info.n_z
    );
    if output_format == OutputFormat::Acdc {
        println!(
            "Total frames to read: {} ({} stacked channel TIFFs)",
            summary.total_frames,
            selection.pos_indices.len() * selection.channel_indices.len()
        );
    } else {
        println!("Total frames to write: {}", summary.total_frames);
    }
    println!();
    println!("Positions:");
    println!(
        "  {}",
        selection
            .pos_indices
            .iter()
            .map(|index| output_format.position_label(*index))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!();
    println!("Timepoints (original indices):");
    println!("  {:?}", selection.time_indices);
    println!();
    println!("Channels (original indices):");
    println!("  {:?}", selection.channel_indices);
    println!();
    println!("Z-slices (original indices):");
    println!("  {:?}", selection.z_indices);
    println!();

    if !yes && !confirm("Proceed with conversion?")? {
        eprintln!("Aborted.");
        std::process::exit(130);
    }

    let mut reporter = SimpleProgress { last_done: 0 };
    let mut progress: Option<&mut dyn ProgressCallback> = Some(&mut reporter);
    run_convert(
        &input_path,
        position,
        time,
        channel,
        z,
        output,
        output_format,
        &mut progress,
    )?;
    eprintln!();
    Ok(())
}

fn run_metadata_command(input_file: &PathBuf, output: Option<&PathBuf>, raw: bool) -> Result<()> {
    let input_path = open_input(input_file)?;
    let content = run_metadata(&input_path, output.map(|path| path.as_path()), raw)?;
    if output.is_none() {
        print!("{content}");
    } else {
        println!("Wrote {}", output.unwrap().display());
    }
    Ok(())
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N]: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

struct SimpleProgress {
    last_done: usize,
}

impl ProgressCallback for SimpleProgress {
    fn on_progress(&mut self, event: ProgressEvent) {
        match event.phase {
            ProgressPhase::Start => {
                self.last_done = 0;
                eprint!("\r{}", event.message);
            }
            ProgressPhase::Advance => {
                if event.done > self.last_done {
                    eprint!(
                        "\r{} [{}/{}]",
                        event.message, event.done, event.total
                    );
                    self.last_done = event.done;
                }
            }
            ProgressPhase::Finish => {
                eprintln!("\r{}", event.message);
            }
        }
    }
}
