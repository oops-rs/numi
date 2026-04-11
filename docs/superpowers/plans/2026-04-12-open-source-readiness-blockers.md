# Open-Source Readiness Blockers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the publish-time embedded assets for `numi-core` and `numi` crate-local so the first public release does not depend on files outside each crate root.

**Architecture:** Keep behavior unchanged and fix the publish boundary instead. `numi-core` will own crate-local copies of its built-in templates, `numi-cli` will own a crate-local copy of its starter config, and regression tests will lock in the invariant that `include_str!` only points inside each crate.

**Tech Stack:** Rust workspace, Cargo packaging, integration/unit tests

---

### Task 1: Make `numi-core` own its embedded built-in templates

**Files:**
- Create: `crates/numi-core/templates/swift/swiftui-assets.jinja`
- Create: `crates/numi-core/templates/swift/l10n.jinja`
- Create: `crates/numi-core/templates/swift/files.jinja`
- Create: `crates/numi-core/templates/objc/assets.jinja`
- Create: `crates/numi-core/templates/objc/l10n.jinja`
- Create: `crates/numi-core/templates/objc/files.jinja`
- Modify: `crates/numi-core/src/render.rs`
- Modify: `crates/numi-core/src/lib.rs`

- [ ] **Step 1: Write the failing regression test for crate-local template embeds**

Add this test module at the end of `crates/numi-core/src/lib.rs`:

```rust
#[cfg(test)]
mod publish_invariants {
    #[test]
    fn builtin_templates_are_embedded_from_within_the_crate() {
        let render_rs = include_str!("render.rs");

        for needle in [
            "include_str!(\"../templates/swift/swiftui-assets.jinja\")",
            "include_str!(\"../templates/swift/l10n.jinja\")",
            "include_str!(\"../templates/swift/files.jinja\")",
            "include_str!(\"../templates/objc/assets.jinja\")",
            "include_str!(\"../templates/objc/l10n.jinja\")",
            "include_str!(\"../templates/objc/files.jinja\")",
        ] {
            assert!(
                render_rs.contains(needle),
                "expected render.rs to contain {needle}"
            );
        }

        assert!(
            !render_rs.contains("../../../templates/"),
            "render.rs should not reference templates outside the crate root"
        );
    }
}
```

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `cargo test -p numi-core builtin_templates_are_embedded_from_within_the_crate -- --exact`

Expected: FAIL because `crates/numi-core/src/render.rs` still references `../../../templates/...`.

- [ ] **Step 3: Copy the built-in templates into the crate-local directory**

Create the new files under `crates/numi-core/templates/` by copying the current repository-level templates byte-for-byte:

```text
templates/swift/swiftui-assets.jinja -> crates/numi-core/templates/swift/swiftui-assets.jinja
templates/swift/l10n.jinja           -> crates/numi-core/templates/swift/l10n.jinja
templates/swift/files.jinja          -> crates/numi-core/templates/swift/files.jinja
templates/objc/assets.jinja          -> crates/numi-core/templates/objc/assets.jinja
templates/objc/l10n.jinja            -> crates/numi-core/templates/objc/l10n.jinja
templates/objc/files.jinja           -> crates/numi-core/templates/objc/files.jinja
```

Do not change template contents in this task.

- [ ] **Step 4: Update `render.rs` to embed the crate-local copies**

Replace the existing constants in `crates/numi-core/src/render.rs`:

```rust
const SWIFTUI_ASSETS_TEMPLATE: &str = include_str!("../templates/swift/swiftui-assets.jinja");
const L10N_TEMPLATE: &str = include_str!("../templates/swift/l10n.jinja");
const FILES_TEMPLATE: &str = include_str!("../templates/swift/files.jinja");
const OBJC_ASSETS_TEMPLATE: &str = include_str!("../templates/objc/assets.jinja");
const OBJC_L10N_TEMPLATE: &str = include_str!("../templates/objc/l10n.jinja");
const OBJC_FILES_TEMPLATE: &str = include_str!("../templates/objc/files.jinja");
```

- [ ] **Step 5: Run the focused test to verify it passes**

Run: `cargo test -p numi-core builtin_templates_are_embedded_from_within_the_crate -- --exact`

Expected: PASS.

- [ ] **Step 6: Run the `numi-core` test suite**

Run: `cargo test -p numi-core`

Expected: all `numi-core` tests pass.

- [ ] **Step 7: Commit the `numi-core` blocker fix**

```bash
git add crates/numi-core/src/lib.rs crates/numi-core/src/render.rs crates/numi-core/templates
git commit -m "fix(numi-core): embed built-in templates from crate-local assets"
```

### Task 2: Make `numi-cli` own its embedded starter config

**Files:**
- Create: `crates/numi-cli/assets/starter-numi.toml`
- Modify: `crates/numi-cli/src/lib.rs`
- Modify: `crates/numi-cli/tests/package_metadata.rs`

- [ ] **Step 1: Write the failing regression test for the crate-local starter config**

Extend `crates/numi-cli/tests/package_metadata.rs` with this test:

```rust
#[test]
fn starter_config_is_embedded_from_within_the_cli_crate() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lib_rs = std::fs::read_to_string(manifest_dir.join("src/lib.rs"))
        .expect("failed to read src/lib.rs");

    assert!(
        lib_rs.contains("include_str!(\"../assets/starter-numi.toml\")"),
        "expected src/lib.rs to embed the crate-local starter config"
    );
    assert!(
        !lib_rs.contains("../../../docs/examples/starter-numi.toml"),
        "src/lib.rs should not reference starter config outside the crate root"
    );
}
```

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `cargo test -p numi starter_config_is_embedded_from_within_the_cli_crate -- --exact`

Expected: FAIL because `crates/numi-cli/src/lib.rs` still references `../../../docs/examples/starter-numi.toml`.

- [ ] **Step 3: Copy the starter config into the CLI crate**

Create `crates/numi-cli/assets/starter-numi.toml` by copying the current contents of `docs/examples/starter-numi.toml` without modification.

- [ ] **Step 4: Update the embed path in `crates/numi-cli/src/lib.rs`**

Change the constant to:

```rust
const STARTER_CONFIG_FALLBACK: &str = include_str!("../assets/starter-numi.toml");
```

- [ ] **Step 5: Run the focused test to verify it passes**

Run: `cargo test -p numi starter_config_is_embedded_from_within_the_cli_crate -- --exact`

Expected: PASS.

- [ ] **Step 6: Run the CLI crate tests**

Run: `cargo test -p numi`

Expected: all `numi` crate tests pass.

- [ ] **Step 7: Commit the `numi-cli` blocker fix**

```bash
git add crates/numi-cli/src/lib.rs crates/numi-cli/tests/package_metadata.rs crates/numi-cli/assets/starter-numi.toml
git commit -m "fix(numi): embed starter config from crate-local assets"
```

### Task 3: Verify packaging and workspace health after the blocker fixes

**Files:**
- Modify: `docs/crates-io-release.md`

- [ ] **Step 1: Update the release doc with the new packaging invariant**

Add a short note to `docs/crates-io-release.md` near the preflight section:

```md
For crates that embed compile-time assets with `include_str!`, keep those assets under the owning crate directory so `cargo package` includes them automatically. Do not point embedded asset paths at repository-level files outside the crate root.
```

- [ ] **Step 2: Run formatting**

Run: `cargo fmt --all --check`

Expected: exit code `0`.

- [ ] **Step 3: Run linting**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: exit code `0`.

- [ ] **Step 4: Run the full test suite**

Run: `cargo test --workspace`

Expected: all workspace tests pass.

- [ ] **Step 5: Verify packaged file lists for the asset-owning crates**

Run: `cargo package --list -p numi-core`

Expected: the output includes:

```text
templates/swift/swiftui-assets.jinja
templates/swift/l10n.jinja
templates/swift/files.jinja
templates/objc/assets.jinja
templates/objc/l10n.jinja
templates/objc/files.jinja
```

Run: `cargo package --list -p numi`

Expected: the output includes:

```text
assets/starter-numi.toml
```

- [ ] **Step 6: Re-run the leaf publish preflight**

Run: `cargo package -p numi-diagnostics --allow-dirty`

Expected: PASS.

Run: `cargo publish --dry-run -p numi-diagnostics --allow-dirty`

Expected: PASS with the dry-run upload warning only.

- [ ] **Step 7: Commit the verification and release-doc update**

```bash
git add docs/crates-io-release.md
git commit -m "docs: document crate-local embedded asset packaging"
```
