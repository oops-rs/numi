use criterion::{Criterion, criterion_group, criterion_main};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

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

fn benchmark_generate_assets(c: &mut Criterion) {
    let temp_root = make_temp_dir("pipeline-assets");
    let fixture_root = repo_root().join("fixtures/xcassets-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);
    let config_path = working_root.join("swiftgen.toml");

    numi_core::generate(&config_path, None).expect("fixture warm-up generate should succeed");

    c.bench_function("generate_assets_fixture", |b| {
        b.iter(|| numi_core::generate(&config_path, None).expect("fixture generate should work"));
    });

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

criterion_group!(benches, benchmark_generate_assets);
criterion_main!(benches);
