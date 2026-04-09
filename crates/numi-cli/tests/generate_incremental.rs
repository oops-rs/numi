use clap::Parser;
use numi_cli::cli::{Cli, Command, WorkspaceSubcommand};

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
fn workspace_generate_accepts_incremental_override_flags() {
    let cli = Cli::try_parse_from([
        "numi",
        "workspace",
        "generate",
        "--workspace",
        "numi-workspace.toml",
        "--no-incremental",
        "--member",
        "ios",
    ])
    .expect("workspace generate command should parse");

    let Command::Workspace(workspace) = cli.command.expect("command should parse") else {
        panic!("expected workspace command");
    };
    let WorkspaceSubcommand::Generate(args) = workspace.command else {
        panic!("expected workspace generate command");
    };

    assert_eq!(
        args.workspace.as_deref(),
        Some(std::path::Path::new("numi-workspace.toml"))
    );
    assert_eq!(args.members, vec!["ios"]);
    assert_eq!(args.incremental_override.resolve(), Some(false));
}
