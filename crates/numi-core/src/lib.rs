mod context;
mod generation_cache;
mod output;
mod parse_cache;
pub mod parse_files;
mod parse_fonts;
mod parse_l10n;
mod parse_xcassets;
mod pipeline;
mod render;

pub use output::WriteOutcome;
pub use pipeline::{
    CheckReport, DumpContextReport, GenerateError, GenerateOptions, GenerateReport, JobReport,
    check, check_loaded_config, dump_context, generate, generate_loaded_config,
    generate_with_options,
};

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
