use clap::Parser;
use numi_cli::cli::{Cli, Command};

#[test]
fn generate_accepts_incremental_override_flags() {
    let cli = Cli::try_parse_from([
        "numi",
        "generate",
        "--config",
        "numi.toml",
        "--incremental",
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
    assert_eq!(args.incremental_override.resolve(), Some(true));
}

#[test]
fn generate_accepts_workspace_flag_with_incremental_override_flags() {
    let cli = Cli::try_parse_from([
        "numi",
        "generate",
        "--workspace",
        "--no-incremental",
        "--job",
        "ios",
    ])
    .expect("generate command should parse");

    let Command::Generate(args) = cli.command.expect("command should parse") else {
        panic!("expected generate command");
    };

    assert!(args.workspace, "workspace flag should be enabled");
    assert_eq!(args.jobs, vec!["ios"]);
    assert_eq!(args.incremental_override.resolve(), Some(false));
}
