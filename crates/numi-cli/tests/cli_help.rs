use clap::{CommandFactory, Parser};
use numi_cli::cli::Cli;
use std::process::Command as ProcessCommand;

fn assert_contains_line_all(stdout: &str, snippets: &[&str]) {
    assert!(
        stdout
            .lines()
            .any(|line| snippets.iter().all(|snippet| line.contains(snippet))),
        "help output did not contain a line with all of {snippets:?}:\n{stdout}"
    );
}

fn assert_contains_all(stdout: &str, snippets: &[&str]) {
    for snippet in snippets {
        assert!(
            stdout.contains(snippet),
            "help output did not contain `{snippet}`:\n{stdout}"
        );
    }
}

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
fn cli_binary_help_shows_expected_top_level_help() {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_numi"))
        .arg("--help")
        .output()
        .expect("failed to run numi --help");

    assert!(output.status.success(), "help command failed");

    let stdout = String::from_utf8(output.stdout).expect("help output was not utf8");
    assert_contains_all(
        &stdout,
        &[
            "Generate Swift code from Apple project resources",
            "Examples:",
            "numi init",
            "numi generate",
            "numi check",
            "numi generate --workspace",
            "numi dump-context --job l10n",
        ],
    );
    assert_contains_line_all(
        &stdout,
        &["generate", "Generate outputs for one config or workspace"],
    );
    assert_contains_line_all(
        &stdout,
        &["check", "Check whether generated outputs are up to date"],
    );
    assert_contains_line_all(
        &stdout,
        &["init", "Write a starter numi.toml in the current directory"],
    );
    assert_contains_line_all(
        &stdout,
        &["dump-context", "Print the template context for a single job"],
    );
}

#[test]
fn cli_binary_help_shows_generate_help() {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--help"])
        .output()
        .expect("failed to run numi generate --help");

    assert!(output.status.success(), "help command failed");

    let stdout = String::from_utf8(output.stdout).expect("help output was not utf8");
    assert_contains_all(
        &stdout,
        &[
            "Generate outputs for one config or workspace",
            "--config <CONFIG>",
            "Use a specific numi.toml instead of auto-discovery",
            "--workspace",
            "Use the ancestor workspace manifest instead of the nearest member manifest",
            "--job <JOBS>",
            "Limit generation to the selected job name",
            "--incremental",
            "Force incremental parsing when supported",
            "--no-incremental",
            "Disable incremental parsing even when the config enables it",
            "numi generate",
            "numi generate --job assets --job l10n",
            "numi generate --workspace",
        ],
    );
}

#[test]
fn cli_binary_help_shows_check_help() {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_numi"))
        .args(["check", "--help"])
        .output()
        .expect("failed to run numi check --help");

    assert!(output.status.success(), "help command failed");

    let stdout = String::from_utf8(output.stdout).expect("help output was not utf8");
    assert_contains_all(
        &stdout,
        &[
            "Check whether generated outputs are up to date",
            "--config <CONFIG>",
            "--workspace",
            "--job <JOBS>",
            "numi check",
            "numi check --job l10n",
            "numi check --workspace",
        ],
    );
}

#[test]
fn cli_binary_help_shows_dump_context_help() {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_numi"))
        .args(["dump-context", "--help"])
        .output()
        .expect("failed to run numi dump-context --help");

    assert!(output.status.success(), "help command failed");

    let stdout = String::from_utf8(output.stdout).expect("help output was not utf8");
    assert_contains_all(
        &stdout,
        &[
            "Print the template context for a single job",
            "--config <CONFIG>",
            "--job <JOB>",
            "Job name to render as JSON context",
            "numi dump-context --job l10n",
            "numi dump-context --config AppUI/numi.toml --job assets",
        ],
    );
}
