use clap::Parser;

fn main() {
    let cli = numi_cli::cli::Cli::parse();
    if let Err(error) = numi_cli::run(cli) {
        eprintln!("{error}");
        std::process::exit(error.exit_code());
    }
}
