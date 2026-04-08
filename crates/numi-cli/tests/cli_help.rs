use clap::{CommandFactory, Parser};
use numi_cli::cli::Cli;
use std::process::Command as ProcessCommand;

#[test]
fn cli_help_lists_expected_commands() {
    let command = Cli::command();
    let names: Vec<_> = command
        .get_subcommands()
        .map(|sub| sub.get_name())
        .collect();
    assert_eq!(
        names,
        ["generate", "check", "init", "config", "dump-context"]
    );
}

#[test]
fn cli_requires_a_subcommand() {
    let err = Cli::try_parse_from(["numi"]).expect_err("expected missing subcommand");
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingSubcommand);
}

#[test]
fn cli_binary_help_lists_expected_commands() {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_numi"))
        .arg("--help")
        .output()
        .expect("failed to run numi --help");

    assert!(output.status.success(), "help command failed");

    let stdout = String::from_utf8(output.stdout).expect("help output was not utf8");
    for command in ["generate", "check", "init", "config", "dump-context"] {
        assert!(
            stdout.contains(command),
            "help output did not list `{command}`:\n{stdout}"
        );
    }
}
