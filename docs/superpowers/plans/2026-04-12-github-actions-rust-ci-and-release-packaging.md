# GitHub Actions Rust CI And Release Packaging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add GitHub Actions Rust CI, GitHub Release binary packaging, and Homebrew tap automation for `numi` while keeping crates.io publication manual.

**Architecture:** Add one lightweight Rust CI workflow for normal development checks and one release workflow modeled on `langcodec` that builds release archives, uploads them to GitHub Releases, and optionally updates the `oops-rs/homebrew-tap` repository. Keep Homebrew source-based by extending the shared tap script and adding a `numi` formula that installs from `crates/numi-cli`.

**Tech Stack:** GitHub Actions YAML, Rust/Cargo, shell scripting, Homebrew formula Ruby

---

### Task 1: Add the repository Rust CI workflow

**Files:**
- Create: `.github/workflows/rust-ci.yml`

- [ ] **Step 1: Write the failing workflow shape**

Create `.github/workflows/rust-ci.yml` with this content:

```yaml
name: Rust CI

on:
  push:
    branches: ["main"]
  pull_request:
    branches: ["main"]

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always

jobs:
  rust-ci:
    runs-on: ubuntu-24.04

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust (stable)
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable
          components: rustfmt, clippy

      - name: Cache Cargo artifacts
        uses: Swatinem/rust-cache@v2

      - name: Check formatting
        run: cargo fmt --all --check

      - name: Run clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: Run tests
        run: cargo test --workspace
```

- [ ] **Step 2: Run local repository verification to confirm the workflow commands are valid for this repo**

Run: `cargo fmt --all --check`
Expected: exit code `0`

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit code `0`

Run: `cargo test --workspace`
Expected: all workspace tests pass

- [ ] **Step 3: Review the workflow against `langcodec` patterns**

Compare `.github/workflows/rust-ci.yml` to `/Users/wendell/developer/oops-rs/langcodec/.github/workflows/rust.yml` and `/Users/wendell/developer/oops-rs/langcodec/.github/workflows/clippy-check.yml`.

Expected:
- stable toolchain install uses `dtolnay/rust-toolchain`
- workflow remains simpler than `langcodec` by keeping fmt, clippy, and tests in one file
- no missing permissions or checkout step

- [ ] **Step 4: Commit the CI workflow**

```bash
git add .github/workflows/rust-ci.yml
git commit -m "ci: add rust validation workflow"
```

### Task 2: Add the GitHub Release binary packaging workflow

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Write the release workflow file**

Create `.github/workflows/release.yml` with this content:

```yaml
name: Release

on:
  release:
    types:
      - published
  workflow_dispatch:
    inputs:
      tag_name:
        description: Existing release tag to build and upload assets for
        required: true
        type: string

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always
  RELEASE_TAG: ${{ github.event_name == 'workflow_dispatch' && inputs.tag_name || github.ref_name }}

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: ubuntu-24.04
            target: x86_64-unknown-linux-gnu
            archive_ext: tar.gz
            binary_name: numi
            binary_asset_ext: ""
          - os: macos-15
            target: x86_64-apple-darwin
            archive_ext: tar.gz
            binary_name: numi
            binary_asset_ext: ""
          - os: macos-15
            target: aarch64-apple-darwin
            archive_ext: tar.gz
            binary_name: numi
            binary_asset_ext: ""
          - os: windows-2022
            target: x86_64-pc-windows-msvc
            archive_ext: zip
            binary_name: numi.exe
            binary_asset_ext: ".exe"

    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ env.RELEASE_TAG }}

      - name: Install Rust (stable)
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable
          targets: ${{ matrix.target }}

      - name: Cache Cargo artifacts
        uses: Swatinem/rust-cache@v2

      - name: Build release binary
        run: cargo build --release -p numi --target ${{ matrix.target }}

      - name: Package artifact (Unix)
        if: runner.os != 'Windows'
        shell: bash
        run: |
          set -euo pipefail
          version="${RELEASE_TAG}"
          archive="numi-${version}-${{ matrix.target }}.${{ matrix.archive_ext }}"
          binary_dir="dist/numi-${version}-${{ matrix.target }}"
          binary_asset="numi-${version}-${{ matrix.target }}"
          mkdir -p "${binary_dir}"
          cp "target/${{ matrix.target }}/release/${{ matrix.binary_name }}" "${binary_dir}/numi"
          cp "target/${{ matrix.target }}/release/${{ matrix.binary_name }}" "${binary_asset}"
          chmod 755 "${binary_asset}"
          tar -C dist -czf "${archive}" "numi-${version}-${{ matrix.target }}"
          shasum -a 256 "${archive}" > "${archive}.sha256"
          shasum -a 256 "${binary_asset}" > "${binary_asset}.sha256"

      - name: Package artifact (Windows)
        if: runner.os == 'Windows'
        shell: pwsh
        run: |
          $version = $env:RELEASE_TAG
          $folder = "numi-$version-${{ matrix.target }}"
          $archive = "$folder.${{ matrix.archive_ext }}"
          $binaryAsset = "$folder.exe"
          New-Item -ItemType Directory -Force -Path "dist/$folder" | Out-Null
          Copy-Item "target/${{ matrix.target }}/release/${{ matrix.binary_name }}" "dist/$folder/numi.exe"
          Copy-Item "target/${{ matrix.target }}/release/${{ matrix.binary_name }}" $binaryAsset
          Compress-Archive -Path "dist/$folder" -DestinationPath $archive
          $hash = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLower()
          "$hash  $archive" | Out-File -Encoding ascii "$archive.sha256"
          $binaryHash = (Get-FileHash -Algorithm SHA256 $binaryAsset).Hash.ToLower()
          "$binaryHash  $binaryAsset" | Out-File -Encoding ascii "$binaryAsset.sha256"

      - name: Upload packaged artifacts
        uses: actions/upload-artifact@v4
        with:
          name: release-${{ matrix.target }}
          path: |
            numi-${{ env.RELEASE_TAG }}-${{ matrix.target }}.${{ matrix.archive_ext }}
            numi-${{ env.RELEASE_TAG }}-${{ matrix.target }}.${{ matrix.archive_ext }}.sha256
            numi-${{ env.RELEASE_TAG }}-${{ matrix.target }}${{ matrix.binary_asset_ext }}
            numi-${{ env.RELEASE_TAG }}-${{ matrix.target }}${{ matrix.binary_asset_ext }}.sha256

  release:
    name: Publish GitHub Release
    runs-on: ubuntu-24.04
    needs: build

    steps:
      - name: Download packaged artifacts
        uses: actions/download-artifact@v4
        with:
          path: dist
          merge-multiple: true

      - name: Publish release and upload binaries
        uses: softprops/action-gh-release@v2
        with:
          generate_release_notes: true
          tag_name: ${{ env.RELEASE_TAG }}
          files: dist/*

  update-homebrew-tap:
    name: Update Homebrew tap
    runs-on: ubuntu-24.04
    needs: release
    env:
      HOMEBREW_TAP_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}

    steps:
      - name: Skip when tap token is unavailable
        if: ${{ env.HOMEBREW_TAP_TOKEN == '' }}
        run: echo "HOMEBREW_TAP_TOKEN is not configured; skipping tap update."

      - name: Check out tap repository
        if: ${{ env.HOMEBREW_TAP_TOKEN != '' }}
        uses: actions/checkout@v4
        with:
          repository: oops-rs/homebrew-tap
          ref: main
          token: ${{ env.HOMEBREW_TAP_TOKEN }}
          path: tap

      - name: Update numi formula
        if: ${{ env.HOMEBREW_TAP_TOKEN != '' }}
        run: bash tap/scripts/update-formula.sh numi "${RELEASE_TAG}"

      - name: Commit updated formula
        if: ${{ env.HOMEBREW_TAP_TOKEN != '' }}
        run: |
          if git -C tap diff --quiet -- Formula; then
            echo "numi formula already up to date"
            exit 0
          fi

          git -C tap config user.name "github-actions[bot]"
          git -C tap config user.email "41898282+github-actions[bot]@users.noreply.github.com"
          git -C tap add Formula
          git -C tap commit -m "numi ${RELEASE_TAG}"
          git -C tap push origin HEAD:main
```

- [ ] **Step 2: Verify the workflow matches the intended release archive naming**

Check `.github/workflows/release.yml` manually.

Expected:
- archive names include `numi-${RELEASE_TAG}-${target}`
- Unix archives are `.tar.gz`
- Windows archive is `.zip`
- raw binary assets and `.sha256` files are included

- [ ] **Step 3: Verify the workflow matches `langcodec` where reuse is intentional**

Compare `.github/workflows/release.yml` with `/Users/wendell/developer/oops-rs/langcodec/.github/workflows/release.yml`.

Expected:
- same trigger pattern: `release.published` plus `workflow_dispatch`
- same release upload flow with `softprops/action-gh-release`
- same optional tap update pattern gated by `HOMEBREW_TAP_TOKEN`
- repository, binary names, crate package, and formula names updated for `numi`

- [ ] **Step 4: Commit the release workflow**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release packaging workflow"
```

### Task 3: Extend the Homebrew tap for `numi`

**Files:**
- Modify: `/Users/wendell/developer/oops-rs/homebrew-tap/scripts/update-formula.sh`
- Create: `/Users/wendell/developer/oops-rs/homebrew-tap/Formula/numi.rb`

- [ ] **Step 1: Write the failing formula-support changes in the tap script**

Update `/Users/wendell/developer/oops-rs/homebrew-tap/scripts/update-formula.sh` in three places.

Extend the usage examples:

```bash
Examples:
  scripts/update-formula.sh grapha v0.1.1
  scripts/update-formula.sh langcodec-cli v0.11.0
  scripts/update-formula.sh numi v0.1.0
```

Add a `numi)` branch inside `render_formula()`:

```bash
    numi)
      cat <<EOF
# This file is auto-generated by scripts/update-formula.sh.
class ${class_name} < Formula
  desc "CLI for generating code from Apple project resources"
  homepage "https://github.com/oops-rs/numi"
  url "https://github.com/oops-rs/numi/archive/refs/tags/${formula_tag}.tar.gz"
  sha256 "${archive_sha}"
  license "MIT"
  head "https://github.com/oops-rs/numi.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", *std_cargo_args(path: "crates/numi-cli")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/numi --version")
  end
end
EOF
      ;;
```

Add a `numi)` branch inside the repository mapping case:

```bash
  numi) repo="oops-rs/numi" ;;
```

- [ ] **Step 2: Run the script to generate the `numi` formula**

Run: `bash /Users/wendell/developer/oops-rs/homebrew-tap/scripts/update-formula.sh numi v0.1.0`

Expected:
- `/Users/wendell/developer/oops-rs/homebrew-tap/Formula/numi.rb` is created
- the formula points at `https://github.com/oops-rs/numi/archive/refs/tags/v0.1.0.tar.gz`
- the install stanza uses `cargo install --locked` with `path: "crates/numi-cli"`

- [ ] **Step 3: Inspect the generated formula**

The generated `/Users/wendell/developer/oops-rs/homebrew-tap/Formula/numi.rb` should be:

```ruby
# This file is auto-generated by scripts/update-formula.sh.
class Numi < Formula
  desc "CLI for generating code from Apple project resources"
  homepage "https://github.com/oops-rs/numi"
  url "https://github.com/oops-rs/numi/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "<generated by script>"
  license "MIT"
  head "https://github.com/oops-rs/numi.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", *std_cargo_args(path: "crates/numi-cli")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/numi --version")
  end
end
```

Expected:
- class name is `Numi`
- no changes are made to unrelated tap formulas

- [ ] **Step 4: Commit the tap updates**

```bash
git -C /Users/wendell/developer/oops-rs/homebrew-tap add scripts/update-formula.sh Formula/numi.rb
git -C /Users/wendell/developer/oops-rs/homebrew-tap commit -m "feat: add numi formula support"
```

### Task 4: Document the release split in `numi`

**Files:**
- Modify: `docs/crates-io-release.md`

- [ ] **Step 1: Add the GitHub Release and Homebrew note**

Insert this section after the preflight section in `docs/crates-io-release.md`:

```md
## GitHub Releases And Homebrew

GitHub Releases package prebuilt `numi` binaries for the supported release targets and attach them as release assets.

When `HOMEBREW_TAP_TOKEN` is configured in GitHub Actions, the release workflow also updates the `oops-rs/homebrew-tap` formula for `numi` from the tagged source release.

crates.io publication remains manual and follows the workspace publish order described below.
```

- [ ] **Step 2: Run repository verification after the docs and workflow changes**

Run: `cargo fmt --all --check`
Expected: exit code `0`

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit code `0`

Run: `cargo test --workspace`
Expected: all workspace tests pass

- [ ] **Step 3: Verify the new files exist**

Run: `find .github/workflows -maxdepth 2 -type f | sort`
Expected output includes:
- `.github/workflows/release.yml`
- `.github/workflows/rust-ci.yml`

Run: `find /Users/wendell/developer/oops-rs/homebrew-tap/Formula -maxdepth 1 -type f | sort`
Expected output includes:
- `/Users/wendell/developer/oops-rs/homebrew-tap/Formula/numi.rb`

- [ ] **Step 4: Commit the release docs update**

```bash
git add docs/crates-io-release.md
git commit -m "docs: clarify release binary and homebrew automation"
```
