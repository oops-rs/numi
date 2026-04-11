# crates.io Release Order

Numi publishes as a small workspace crate family, but the intended end-user install path is:

```bash
cargo install numi
```

## Why `cargo publish --dry-run -p numi` still fails locally

The `numi` package depends on other workspace crates:

- `numi-diagnostics`
- `numi-ir`
- `numi-config`
- `numi-core`

Those dependencies are declared as `path + version` dependencies, which is the correct Cargo shape for publishing. During `cargo publish --dry-run`, Cargo prepares the upload as if the package were being resolved from crates.io. That means a dependent crate can only pass dry-run once its internal dependencies already exist in the registry index.

For the first release of this workspace, that creates a topological publish requirement:

1. Publish leaf crates first.
2. Wait for each published crate to appear in the crates.io index.
3. Publish the next layer of crates.

This is why these outcomes are expected before the first real publish:

- `cargo publish --dry-run -p numi-diagnostics` passes.
- `cargo publish --dry-run -p numi-ir` fails until `numi-diagnostics` is published.
- `cargo publish --dry-run -p numi-config` fails until `numi-diagnostics` is published.
- `cargo publish --dry-run -p numi-core` fails until `numi-ir` and `numi-config` are published.
- `cargo publish --dry-run -p numi` fails until `numi-core` and `numi-config` are published.

## Dependency Order

The release order for the initial crates.io publish is:

```text
numi-diagnostics
numi-ir
numi-config
numi-core
numi
```

`numi-ir` and `numi-config` both depend only on `numi-diagnostics`, so they can be swapped if needed. `numi-core` must come after both of them, and `numi` must come last.

## Preflight

Run the normal local verification first:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Then confirm the leaf crate can be packaged and dry-run published:

```bash
cargo package -p numi-diagnostics --allow-dirty
cargo publish --dry-run -p numi-diagnostics
```

If those pass, failures in downstream crates that mention missing unpublished internal crates are expected for the initial release. They are not manifest-shape failures.

## First Release Sequence

Publish one crate at a time and wait for the index to catch up between steps.

```bash
cargo publish -p numi-diagnostics
cargo info numi-diagnostics

cargo publish -p numi-ir
cargo info numi-ir

cargo publish -p numi-config
cargo info numi-config

cargo publish -p numi-core
cargo info numi-core

cargo publish -p numi
cargo info numi
```

Use `cargo info <crate>` as a simple check that the previous crate has propagated to the registry before you publish the next one.

## After The First Release

Once all internal crates already exist on crates.io, `cargo publish --dry-run -p numi` becomes a meaningful end-to-end dry-run for later releases.
