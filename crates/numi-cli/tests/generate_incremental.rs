use clap::Parser;
use numi_cli::cli::{Cli, Command};

#[test]
fn generate_accepts_incremental_mode_always() {
    let cli = Cli::try_parse_from([
        "numi",
        "generate",
        "--config",
        "numi.toml",
        "--incremental",
        "always",
        "--job",
        "files",
    ])
    .expect("generate command should parse");

    let Command::Generate(args) = cli.command.expect("command should parse") else {
        panic!("expected generate command");
    };

    assert_eq!(
        args.config.as_deref(),
        Some(std::path::Path::new("numi.toml"))
    );
    assert_eq!(args.jobs, vec!["files"]);
    assert_eq!(args.incremental_override.resolve().incremental, Some(true));
    assert!(!args.incremental_override.resolve().force_regenerate);
}

#[test]
fn generate_accepts_incremental_mode_refresh() {
    let cli = Cli::try_parse_from([
        "numi",
        "generate",
        "--workspace",
        "--incremental",
        "refresh",
        "--job",
        "files",
    ])
    .expect("generate command should parse");

    let Command::Generate(args) = cli.command.expect("command should parse") else {
        panic!("expected generate command");
    };

    assert_eq!(args.config.as_deref(), None);
    assert!(args.workspace, "workspace flag should be enabled");
    assert_eq!(args.jobs, vec!["files"]);
    assert_eq!(args.incremental_override.resolve().incremental, Some(true));
    assert!(args.incremental_override.resolve().force_regenerate);
}

#[test]
fn generate_accepts_incremental_mode_never() {
    let cli = Cli::try_parse_from(["numi", "generate", "--incremental", "never", "--job", "ios"])
        .expect("generate command should parse");

    let Command::Generate(args) = cli.command.expect("command should parse") else {
        panic!("expected generate command");
    };

    assert_eq!(args.jobs, vec!["ios"]);
    assert_eq!(args.incremental_override.resolve().incremental, Some(false));
    assert!(!args.incremental_override.resolve().force_regenerate);
}
