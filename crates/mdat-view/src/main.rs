use clap::Parser;
use mdat_view::{run, ViewArgs};

#[derive(Parser)]
#[command(name = "mdat-view", about = "Local viewer for microscopy datasets")]
struct Cli {
    #[command(flatten)]
    args: ViewArgs,
}

fn main() {
    let cli = Cli::parse();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    if let Err(e) = rt.block_on(run(cli.args)) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}