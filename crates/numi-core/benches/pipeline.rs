use criterion::{Criterion, criterion_group, criterion_main};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct PreparedFixture {
    temp_root: PathBuf,
    working_root: PathBuf,
}

impl PreparedFixture {
    fn config_path(&self) -> PathBuf {
        self.working_root.join("numi.toml")
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should exist")
}

fn make_temp_dir(bench_name: &str) -> PathBuf {
    let unique = format!(
        "numi-bench-{bench_name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn copy_dir_all(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("destination directory should exist");

    for entry in fs::read_dir(source).expect("source directory should be readable") {
        let entry = entry.expect("directory entry should be readable");
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().expect("file type should be readable");

        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &destination_path);
        } else {
            fs::copy(entry.path(), destination_path).expect("fixture file should copy");
        }
    }
}

fn prepare_fixture(fixture_name: &str, bench_name: &str) -> PreparedFixture {
    let temp_root = make_temp_dir(bench_name);
    let fixture_root = repo_root().join("fixtures").join(fixture_name);
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);
    PreparedFixture {
        temp_root,
        working_root,
    }
}

fn cleanup_fixture(fixture: PreparedFixture) {
    fs::remove_dir_all(fixture.temp_root).expect("temp dir should be removed");
}

fn benchmark_generate_assets_cache_hit_fixture(c: &mut Criterion) {
    let fixture = prepare_fixture("xcassets-basic", "pipeline-assets-basic");
    let config_path = fixture.config_path();
    numi_core::generate(&config_path, None).expect("fixture warm-up generate should succeed");

    c.bench_function("generate_assets_cache_hit_fixture", |b| {
        b.iter(|| numi_core::generate(&config_path, None).expect("fixture generate should work"));
    });

    cleanup_fixture(fixture);
}

fn benchmark_generate_mixed_large_cache_hit_fixture(c: &mut Criterion) {
    let fixture = prepare_fixture("bench-mixed-large", "pipeline-mixed-large");
    let config_path = fixture.config_path();

    numi_core::generate(&config_path, None).expect("fixture warm-up generate should succeed");

    c.bench_function("generate_mixed_large_cache_hit_fixture", |b| {
        b.iter(|| numi_core::generate(&config_path, None).expect("fixture generate should work"));
    });

    cleanup_fixture(fixture);
}

fn benchmark_discover_workspace_from_member_directory(c: &mut Criterion) {
    let fixture = prepare_fixture("multimodule-repo", "pipeline-discover-multimodule");
    let member_root = fixture.working_root.join("apps/assets");
    let discovery_result = numi_config::discover_workspace_ancestor(&member_root, None);
    assert!(
        matches!(
            discovery_result,
            Ok(path)
                if path
                    == fixture
                        .working_root
                        .join("numi.toml")
                        .canonicalize()
                        .expect("workspace manifest should canonicalize")
        ),
        "member directory should discover the repo-root workspace numi.toml"
    );

    c.bench_function("discover_workspace_from_member_directory", |b| {
        b.iter(|| {
            let _ = numi_config::discover_workspace_ancestor(&member_root, None);
        });
    });

    cleanup_fixture(fixture);
}

criterion_group!(
    benches,
    benchmark_generate_assets_cache_hit_fixture,
    benchmark_generate_mixed_large_cache_hit_fixture,
    benchmark_discover_workspace_from_member_directory
);
criterion_main!(benches);
