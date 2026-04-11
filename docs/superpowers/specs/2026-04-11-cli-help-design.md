# CLI Help Enrichment Design

## Summary

Improve `numi --help` so it explains what the tool does, what each command is for, and how to run common workflows. The help output should feel more useful than a bare command list while staying compact enough for terminal use.

## Current Facts

- The top-level CLI help currently uses a generic description: `CLI for numi`.
- The current help output lists commands but does not describe them.
- Most command arguments in `crates/numi-cli/src/cli.rs` do not define explicit help text.
- Clap can render richer help from command metadata without changing command behavior.
- The existing CLI help tests in `crates/numi-cli/tests/cli_help.rs` only assert command presence, not help quality.

## Goal

Make `numi --help` and selected subcommand help screens immediately useful for someone learning the workflow:

- understand what `numi` does
- know when to use each command
- see a few concrete example invocations

## Options Considered

### Option 1: Minimal polish

Add one-line command descriptions only.

Pros:
- smallest change
- lowest maintenance burden

Cons:
- still leaves the CLI feeling sparse
- does not help new users learn the common workflow quickly

### Option 2: Practical help polish

Add stronger top-level descriptions, per-command and per-flag help text, and a few short examples.

Pros:
- meaningfully improves first-run usability
- keeps output compact
- uses built-in Clap help features cleanly

Cons:
- requires more test coverage than a minimal change

### Option 3: Full guided help

Turn help output into mini documentation with long narrative sections for most commands.

Pros:
- richest onboarding experience

Cons:
- risks noisy, hard-to-scan terminal output
- adds more maintenance overhead

## Decision

Use Option 2.

This is the best fit for `numi`: practical, task-oriented help with examples, without turning `--help` into a wall of text.

## Design

### Top-level help

Replace the generic top-level description with user-facing language that explains the job of the tool. Add a short examples block showing the basic workflow:

- `numi init`
- `numi generate`
- `numi check`
- `numi generate --workspace`
- `numi dump-context --job l10n`

### Command help

Add short descriptions for:

- `generate`
- `check`
- `init`
- `config`
- `dump-context`
- `config locate`
- `config print`

These descriptions should explain intent, not implementation details.

### Flag help

Add explicit help text for the flags users are most likely to need:

- `--config`
- `--workspace`
- `--job`
- `--incremental`
- `--no-incremental`
- `--force`

Flag wording should be concrete and match actual behavior already implemented by the command handlers.

### Example placement

Use short `after_help` example sections on:

- top-level `numi --help`
- `generate --help`
- `check --help`
- `dump-context --help`

Keep examples short and copy-pasteable.

## Testing

Use TDD:

1. Extend CLI help tests first to assert the new descriptions and example strings.
2. Run the targeted CLI help tests and watch them fail.
3. Update Clap metadata in `crates/numi-cli/src/cli.rs`.
4. Re-run the targeted tests until they pass.
5. Run any focused follow-up checks needed for the CLI binary help output.

## Non-Goals

- changing command behavior
- adding new commands or flags
- embedding full tutorial content into `--help`
- changing README or external docs as part of this step
