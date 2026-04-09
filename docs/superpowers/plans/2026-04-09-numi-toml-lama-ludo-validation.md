# Numi `numi.toml` And lama-ludo Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Change Numi's discovered config filename to `numi.toml`, then validate Numi against the real lama-ludo iOS project by adding isolated module-local configs that generate Numi-owned comparison outputs.

**Architecture:** Keep the product contract simple: discovery uses only `numi.toml`, while explicit `--config <path>` continues to accept any filename. After the contract rename, add hand-authored module-local configs only in lama-ludo packages that already contain supported inputs, with output paths isolated from the project's existing generated Swift.

**Tech Stack:** Rust workspace (`numi-config`, `numi-cli`, `numi-core`), TOML configs, cargo tests, real-world validation against Swift Package modules in `/Users/wendell/developer/WeNext/lama-ludo-ios`

---

## File Structure

### Numi Contract Files

- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-config/src/discovery.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-config/src/validate.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-config/src/lib.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-cli/src/lib.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-cli/tests/config_commands.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-cli/tests/generate_assets.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-cli/tests/generate_l10n.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-core/src/pipeline.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-core/benches/pipeline.rs`

### Fixtures And Starter Config

- Rename: `/Users/wendell/Developer/oops-rs/numi/docs/examples/starter-swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/docs/examples/starter-numi.toml`
- Rename: `/Users/wendell/Developer/oops-rs/numi/fixtures/l10n-basic/swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/fixtures/l10n-basic/numi.toml`
- Rename: `/Users/wendell/Developer/oops-rs/numi/fixtures/xcassets-basic/swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/fixtures/xcassets-basic/numi.toml`
- Rename: `/Users/wendell/Developer/oops-rs/numi/fixtures/xcstrings-basic/swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/fixtures/xcstrings-basic/numi.toml`
- Rename: `/Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/AppUI/swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/AppUI/numi.toml`
- Rename: `/Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/Core/swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/Core/numi.toml`

### Docs

- Modify: `/Users/wendell/Developer/oops-rs/numi/README.md`
- Modify: `/Users/wendell/Developer/oops-rs/numi/docs/spec.md`
- Modify: `/Users/wendell/Developer/oops-rs/numi/docs/migration-from-swiftgen.md`
- Modify: `/Users/wendell/Developer/oops-rs/numi/docs/context-schema.md`

### lama-ludo Validation Configs

- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Account/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Activity/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Couple/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Family/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Headline/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Level/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Medal/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Message/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Moment/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Operation/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/PK/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Rank/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Room/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Search/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/SeasonPass/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Setting/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Share/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Store/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Wallet/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/WebGame/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/WebView/numi.toml`

### Representative lama-ludo Output Paths

- Create-on-generate: `/Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/Sources/AppResource/Generated/NumiAssets.swift`
- Create-on-generate: `/Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/Sources/AppResource/Generated/NumiL10n.swift`
- Create-on-generate: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/Sources/Game/Generated/NumiAssets.swift`
- Create-on-generate: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/Sources/Game/Generated/NumiL10n.swift`
- Create-on-generate: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/Sources/Profile/Generated/NumiAssets.swift`
- Create-on-generate: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/Sources/Profile/Generated/NumiL10n.swift`

---

### Task 1: Rename The Discovery Contract To `numi.toml`

**Files:**
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-config/src/discovery.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-config/src/validate.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-config/src/lib.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-cli/src/lib.rs`
- Test: `/Users/wendell/Developer/oops-rs/numi/crates/numi-cli/tests/config_commands.rs`

- [ ] **Step 1: Write the failing discovery and init tests around `numi.toml`**

Update the existing CLI tests so they expect:

- nearest ancestor discovery from `numi.toml`
- ambiguous descendant output mentioning `AppUI/numi.toml` and `Core/numi.toml`
- descendant fallback from `numi.toml`
- `init` writing `numi.toml`
- config hints mentioning `numi.toml`

Use this replacement pattern in `/Users/wendell/Developer/oops-rs/numi/crates/numi-cli/tests/config_commands.rs`:

```rust
let config_path = root.join("numi.toml");
assert!(stderr.contains("AppUI/numi.toml"));
assert!(stderr.contains("Core/numi.toml"));
fs::read_to_string(root.join("numi.toml")).expect("starter config should exist");
```

- [ ] **Step 2: Run the focused config test file and verify it fails**

Run:

```bash
cargo test -p numi-cli --test config_commands -v
```

Expected:

- FAIL because the product still discovers `swiftgen.toml`
- FAIL because `init` still writes `swiftgen.toml`

- [ ] **Step 3: Change the discovery constant and CLI starter reference**

Apply the minimal contract rename:

```rust
// crates/numi-config/src/discovery.rs
pub const CONFIG_FILE_NAME: &str = "numi.toml";
```

```rust
// crates/numi-cli/src/lib.rs
const STARTER_CONFIG: &str = include_str!("../../../docs/examples/starter-numi.toml");
```

Update validation and job-selection hints to say `numi.toml`, for example:

```rust
.with_hint("set `version = 1` in numi.toml")
.with_hint("add one `[[jobs]]` table to numi.toml")
.with_hint("select one of the job names declared in numi.toml")
```

- [ ] **Step 4: Re-run the focused config test file**

Run:

```bash
cargo test -p numi-cli --test config_commands -v
```

Expected:

- the discovery and init tests that only depend on the filename contract now pass
- fixture-backed tests may still fail until Task 2 renames the fixture files

- [ ] **Step 5: Commit the contract rename slice**

Run:

```bash
git add crates/numi-config/src/discovery.rs crates/numi-config/src/validate.rs crates/numi-config/src/lib.rs crates/numi-cli/src/lib.rs crates/numi-cli/tests/config_commands.rs
git commit -m "refactor: rename discovered config to numi toml"
```

---

### Task 2: Rename Fixtures, Tests, And Docs To Match The New Contract

**Files:**
- Rename: `/Users/wendell/Developer/oops-rs/numi/docs/examples/starter-swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/docs/examples/starter-numi.toml`
- Rename: `/Users/wendell/Developer/oops-rs/numi/fixtures/l10n-basic/swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/fixtures/l10n-basic/numi.toml`
- Rename: `/Users/wendell/Developer/oops-rs/numi/fixtures/xcassets-basic/swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/fixtures/xcassets-basic/numi.toml`
- Rename: `/Users/wendell/Developer/oops-rs/numi/fixtures/xcstrings-basic/swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/fixtures/xcstrings-basic/numi.toml`
- Rename: `/Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/AppUI/swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/AppUI/numi.toml`
- Rename: `/Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/Core/swiftgen.toml` -> `/Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/Core/numi.toml`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-cli/tests/generate_assets.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-cli/tests/generate_l10n.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-cli/tests/config_commands.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-core/src/pipeline.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/crates/numi-core/benches/pipeline.rs`
- Modify: `/Users/wendell/Developer/oops-rs/numi/README.md`
- Modify: `/Users/wendell/Developer/oops-rs/numi/docs/spec.md`
- Modify: `/Users/wendell/Developer/oops-rs/numi/docs/migration-from-swiftgen.md`
- Modify: `/Users/wendell/Developer/oops-rs/numi/docs/context-schema.md`

- [ ] **Step 1: Rename the actual TOML fixture files and starter example**

Run:

```bash
mv /Users/wendell/Developer/oops-rs/numi/docs/examples/starter-swiftgen.toml /Users/wendell/Developer/oops-rs/numi/docs/examples/starter-numi.toml
mv /Users/wendell/Developer/oops-rs/numi/fixtures/l10n-basic/swiftgen.toml /Users/wendell/Developer/oops-rs/numi/fixtures/l10n-basic/numi.toml
mv /Users/wendell/Developer/oops-rs/numi/fixtures/xcassets-basic/swiftgen.toml /Users/wendell/Developer/oops-rs/numi/fixtures/xcassets-basic/numi.toml
mv /Users/wendell/Developer/oops-rs/numi/fixtures/xcstrings-basic/swiftgen.toml /Users/wendell/Developer/oops-rs/numi/fixtures/xcstrings-basic/numi.toml
mv /Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/AppUI/swiftgen.toml /Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/AppUI/numi.toml
mv /Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/Core/swiftgen.toml /Users/wendell/Developer/oops-rs/numi/fixtures/multimodule-repo/Core/numi.toml
```

- [ ] **Step 2: Replace repo references from `swiftgen.toml` to `numi.toml`**

Update every production-facing reference in the files above. Use this shape in code and docs:

```rust
.args(["generate", "--config", "numi.toml"])
let config_path = working_root.join("numi.toml");
```

```md
numi generate --config AppUI/numi.toml
- Create a starter `numi.toml` in the current directory
```

Also update the starter-config include path:

```rust
include_str!("../../../docs/examples/starter-numi.toml")
```

- [ ] **Step 3: Run the affected CLI and core test targets**

Run:

```bash
cargo test -p numi-cli --test config_commands -v
cargo test -p numi-cli --test generate_assets -v
cargo test -p numi-cli --test generate_l10n -v
cargo test -p numi-core pipeline::tests -- --nocapture
```

Expected:

- PASS for all renamed config-path tests
- PASS for the pipeline fixtures once the renamed fixture files are referenced correctly

- [ ] **Step 4: Run full workspace verification for the rename**

Run:

```bash
cargo test -v
cargo fmt --check
```

Expected:

- PASS
- no remaining product-facing failures that mention `swiftgen.toml` discovery

- [ ] **Step 5: Commit the fixture and doc rename slice**

Run:

```bash
git add docs/examples fixtures README.md docs/spec.md docs/migration-from-swiftgen.md docs/context-schema.md crates/numi-cli/tests crates/numi-core/src/pipeline.rs crates/numi-core/benches/pipeline.rs
git commit -m "docs: rename config contract to numi toml"
```

---

### Task 3: Add Module-Local lama-ludo Validation Configs

**Files:**
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Account/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Activity/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Couple/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Family/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Headline/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Level/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Medal/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Message/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Moment/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Operation/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/PK/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Rank/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Room/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Search/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/SeasonPass/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Setting/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Share/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Store/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Wallet/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/WebGame/numi.toml`
- Create: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/WebView/numi.toml`

- [ ] **Step 1: Create `numi.toml` for the modules that have both assets and localization**

Use this template for:

- `Account`
- `Activity`
- `Couple`
- `Game`
- `Headline`
- `Level`
- `Medal`
- `Operation`
- `Room`
- `Search`
- `SeasonPass`
- `Share`
- `Store`
- `Wallet`
- `WebGame`
- `WebView`

Config body:

```toml
version = 1

[defaults]
access_level = "internal"

[defaults.bundle]
mode = "module"

[[jobs]]
name = "assets"
output = "Sources/<Target>/Generated/NumiAssets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Sources/<Target>/Assets.xcassets"

[jobs.template]
builtin = "swiftui-assets"

[[jobs]]
name = "l10n"
output = "Sources/<Target>/Generated/NumiL10n.swift"

[[jobs.inputs]]
type = "xcstrings"
path = "Sources/<Target>/Localizable.xcstrings"

[jobs.template]
builtin = "l10n"
```

Replace `<Target>` with the module name in each file.

- [ ] **Step 2: Create `numi.toml` for modules that use `Asset.xcassets`**

Use this template for:

- `Message`
- `Profile`

```toml
version = 1

[defaults]
access_level = "internal"

[defaults.bundle]
mode = "module"

[[jobs]]
name = "assets"
output = "Sources/<Target>/Generated/NumiAssets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Sources/<Target>/Asset.xcassets"

[jobs.template]
builtin = "swiftui-assets"

[[jobs]]
name = "l10n"
output = "Sources/<Target>/Generated/NumiL10n.swift"

[[jobs.inputs]]
type = "xcstrings"
path = "Sources/<Target>/Localizable.xcstrings"

[jobs.template]
builtin = "l10n"
```

Use:

- `Message` for `<Target> = Message`
- `Profile` for `<Target> = Profile`

- [ ] **Step 3: Create `numi.toml` for asset-only modules**

Use this template for:

- `Family`
- `Moment`
- `PK`
- `Rank`
- `Setting`

```toml
version = 1

[defaults]
access_level = "internal"

[defaults.bundle]
mode = "module"

[[jobs]]
name = "assets"
output = "Sources/<Target>/Generated/NumiAssets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Sources/<Target>/<AssetFile>"

[jobs.template]
builtin = "swiftui-assets"
```

Use these exact substitutions:

- `Family`: `<Target> = Family`, `<AssetFile> = Assets.xcassets`
- `Moment`: `<Target> = Moment`, `<AssetFile> = Asset.xcassets`
- `PK`: `<Target> = PK`, `<AssetFile> = Assets.xcassets`
- `Rank`: `<Target> = Rank`, `<AssetFile> = Asset.xcassets`
- `Setting`: `<Target> = Setting`, `<AssetFile> = Assets.xcassets`

- [ ] **Step 4: Create the `AppUI` validation config against `AppResource`**

Create `/Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml` with:

```toml
version = 1

[defaults]
access_level = "public"

[defaults.bundle]
mode = "module"

[[jobs]]
name = "assets"
output = "Sources/AppResource/Generated/NumiAssets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Sources/AppResource/Resources/Assets.xcassets"

[jobs.template]
builtin = "swiftui-assets"

[[jobs]]
name = "l10n"
output = "Sources/AppResource/Generated/NumiL10n.swift"

[[jobs.inputs]]
type = "strings"
path = "Sources/AppResource/Resources/en.lproj"

[jobs.template]
builtin = "l10n"
```

- [ ] **Step 5: Commit the real-world config slice**

Run:

```bash
git -C /Users/wendell/developer/WeNext/lama-ludo-ios add AppUI/numi.toml Modules/*/numi.toml
git -C /Users/wendell/developer/WeNext/lama-ludo-ios commit -m "chore: add numi validation configs"
```

Expected:

- one commit in the lama-ludo repo containing only the validation configs

---

### Task 4: Run Real-World Validation Against Representative Modules

**Files:**
- Use: `/Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml`
- Use: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/numi.toml`
- Use: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/numi.toml`
- Use: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Setting/numi.toml`

- [ ] **Step 1: Validate the `.strings` + assets AppUI case**

Run:

```bash
cargo run -p numi-cli -- generate --config /Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml
```

Expected:

- `AppUI/Sources/AppResource/Generated/NumiAssets.swift` is created
- `AppUI/Sources/AppResource/Generated/NumiL10n.swift` is created
- existing `Assets.generated.swift` and `Strings.generated.swift` remain untouched

- [ ] **Step 2: Validate a module with `Assets.xcassets` + `.xcstrings`**

Run:

```bash
cargo run -p numi-cli -- generate --config /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/numi.toml
```

Expected:

- `Modules/Game/Sources/Game/Generated/NumiAssets.swift` is created
- `Modules/Game/Sources/Game/Generated/NumiL10n.swift` is created
- warnings may print if unsupported `.xcstrings` variations exist, but generation still succeeds

- [ ] **Step 3: Validate a module with `Asset.xcassets` + `.xcstrings`**

Run:

```bash
cargo run -p numi-cli -- generate --config /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/numi.toml
```

Expected:

- `Modules/Profile/Sources/Profile/Generated/NumiAssets.swift` is created
- `Modules/Profile/Sources/Profile/Generated/NumiL10n.swift` is created

- [ ] **Step 4: Validate an asset-only module**

Run:

```bash
cargo run -p numi-cli -- generate --config /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Setting/numi.toml
```

Expected:

- `Modules/Setting/Sources/Setting/Generated/NumiAssets.swift` is created
- no localization output is expected because the config has no l10n job

- [ ] **Step 5: Run `check` on the same representative modules**

Run:

```bash
cargo run -p numi-cli -- check --config /Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml
cargo run -p numi-cli -- check --config /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/numi.toml
cargo run -p numi-cli -- check --config /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/numi.toml
cargo run -p numi-cli -- check --config /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Setting/numi.toml
```

Expected:

- exit code `0` after generation
- warnings may be printed for unsupported `.xcstrings` variations

- [ ] **Step 6: Commit validation notes only if code changes were required**

If real-world validation reveals a product bug and you fix it, commit the fix separately:

```bash
git add <exact numi files touched>
git commit -m "fix: handle lama ludo validation path"
```

If no code changes were required, skip this commit.

---

### Task 5: Run The Full lama-ludo Config Sweep And Final Verification

**Files:**
- Use: all `numi.toml` files created in lama-ludo
- Verify: `/Users/wendell/Developer/oops-rs/numi`
- Verify: `/Users/wendell/developer/WeNext/lama-ludo-ios`

- [ ] **Step 1: Run a full generate sweep over every lama-ludo validation config**

Run:

```bash
for config in \
  /Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Account/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Activity/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Couple/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Family/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Headline/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Level/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Medal/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Message/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Moment/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Operation/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/PK/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Rank/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Room/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Search/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/SeasonPass/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Setting/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Share/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Store/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Wallet/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/WebGame/numi.toml \
  /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/WebView/numi.toml
do
  cargo run -p numi-cli -- generate --config "$config" || exit 1
done
```

Expected:

- every config is runnable
- warnings are tolerated
- no config overwrites an existing non-Numi generated file

- [ ] **Step 2: Run the final Numi verification suite**

Run:

```bash
cargo test -v
cargo fmt --check
```

Expected:

- PASS

- [ ] **Step 3: Inspect both repos for unintended churn**

Run:

```bash
git -C /Users/wendell/Developer/oops-rs/numi status --short
git -C /Users/wendell/developer/WeNext/lama-ludo-ios status --short
```

Expected:

- Numi repo shows only the intended contract/doc/test changes
- lama-ludo shows only the new `numi.toml` files and generated Numi-owned comparison outputs

- [ ] **Step 4: Commit final follow-up changes if needed**

If Task 5 required additional Numi fixes:

```bash
git add <exact numi files touched>
git commit -m "fix: complete numi toml validation rollout"
```

If only lama-ludo configs or generated outputs changed, do not mix those into the Numi repo commit.

- [ ] **Step 5: Prepare merge or push handoff**

At this point, gather:

- the final Numi commit(s)
- the lama-ludo config commit
- the representative validation commands that passed
- any warnings observed repeatedly across modules

Use that summary as the basis for the execution handoff or code review request.
