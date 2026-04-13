use std::collections::HashSet;

use numi_diagnostics::Diagnostic;

use crate::model::{
    ACCESS_LEVEL_VALUES, BUILTIN_TEMPLATE_LANGUAGES, BUNDLE_MODE_VALUES, Config, HooksConfig,
    INPUT_KIND_VALUES, TemplateConfig, builtin_template_names_for_language,
};

pub fn validate_config(config: &Config) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if config.version != 1 {
        diagnostics.push(
            Diagnostic::error(format!(
                "unsupported config version `{}`; only version `1` is supported",
                config.version
            ))
            .with_hint("set `version = 1` in numi.toml"),
        );
    }

    if config.jobs.is_empty() {
        diagnostics.push(
            Diagnostic::error("config must define at least one job")
                .with_hint("add one `[jobs.<name>]` table to numi.toml"),
        );
    }

    if let Some(access_level) = config.defaults.access_level.as_deref() {
        validate_allowed_value(
            &mut diagnostics,
            "defaults.access_level",
            access_level,
            ACCESS_LEVEL_VALUES,
            None,
        );
    }

    if let Some(mode) = config.defaults.bundle.mode.as_deref() {
        validate_allowed_value(
            &mut diagnostics,
            "defaults.bundle.mode",
            mode,
            BUNDLE_MODE_VALUES,
            None,
        );
    }

    let mut job_names = HashSet::new();
    for job in &config.jobs {
        if !job_names.insert(job.name.clone()) {
            diagnostics.push(
                Diagnostic::error(format!("duplicate job name `{}`", job.name))
                    .with_job(job.name.clone())
                    .with_hint("rename one of the duplicate jobs so each job name is unique"),
            );
        }

        if job.inputs.is_empty() {
            diagnostics.push(
                Diagnostic::error("job must define at least one input")
                    .with_job(job.name.clone())
                    .with_hint("add one or more `[[jobs.<name>.inputs]]` tables"),
            );
        }

        if let Some(access_level) = job.access_level.as_deref() {
            validate_allowed_value(
                &mut diagnostics,
                "job access_level",
                access_level,
                ACCESS_LEVEL_VALUES,
                Some(job.name.as_str()),
            );
        }

        if let Some(mode) = job.bundle.mode.as_deref() {
            validate_allowed_value(
                &mut diagnostics,
                "job bundle.mode",
                mode,
                BUNDLE_MODE_VALUES,
                Some(job.name.as_str()),
            );
        }

        for input in &job.inputs {
            validate_allowed_value(
                &mut diagnostics,
                "jobs.inputs[].type",
                &input.kind,
                INPUT_KIND_VALUES,
                Some(job.name.as_str()),
            );
        }

        validate_template(
            &mut diagnostics,
            &job.template,
            "job template",
            &format!("jobs.{}.template", job.name),
            Some(job.name.as_str()),
        );
        validate_hooks(
            &mut diagnostics,
            &job.hooks,
            "job hook",
            &format!("jobs.{}.hooks", job.name),
            Some(job.name.as_str()),
        );
    }

    diagnostics
}

pub(crate) fn validate_hooks(
    diagnostics: &mut Vec<Diagnostic>,
    hooks: &HooksConfig,
    label: &str,
    field_path: &str,
    job: Option<&str>,
) {
    validate_hook_command(
        diagnostics,
        hooks
            .pre_generate
            .as_ref()
            .map(|hook| hook.command.as_slice()),
        label,
        &format!("{field_path}.pre_generate.command"),
        job,
    );
    validate_hook_command(
        diagnostics,
        hooks
            .post_generate
            .as_ref()
            .map(|hook| hook.command.as_slice()),
        label,
        &format!("{field_path}.post_generate.command"),
        job,
    );
}

fn validate_hook_command(
    diagnostics: &mut Vec<Diagnostic>,
    command: Option<&[String]>,
    label: &str,
    field_path: &str,
    job: Option<&str>,
) {
    let Some(command) = command else {
        return;
    };

    if command.is_empty() {
        let diagnostic = Diagnostic::error(format!("{label} command must not be empty")).with_hint(
            format!("set `{field_path} = [\"tool\"]` or remove the hook"),
        );
        diagnostics.push(match job {
            Some(job) => diagnostic.with_job(job.to_owned()),
            None => diagnostic,
        });
        return;
    }

    if command[0].trim().is_empty() {
        let diagnostic = Diagnostic::error(format!("{label} executable must not be empty"))
            .with_hint(format!("set a non-empty executable in `{field_path}[0]`"));
        diagnostics.push(match job {
            Some(job) => diagnostic.with_job(job.to_owned()),
            None => diagnostic,
        });
    }
}

pub(crate) fn validate_template(
    diagnostics: &mut Vec<Diagnostic>,
    template: &TemplateConfig,
    label: &str,
    field_path: &str,
    job: Option<&str>,
) {
    let builtin = template.builtin.as_ref();
    let builtin_state = builtin.map_or(BuiltinState::Empty, |builtin| {
        match (builtin.language.as_deref(), builtin.name.as_deref()) {
            (Some(language), Some(name)) => BuiltinState::Complete { language, name },
            (Some(_), None) | (None, Some(_)) => BuiltinState::Partial,
            (None, None) => BuiltinState::Empty,
        }
    });

    let template_sources = usize::from(matches!(builtin_state, BuiltinState::Complete { .. }))
        + usize::from(template.path.is_some());
    if template_sources != 1 {
        let diagnostic = Diagnostic::error(format!("{label} must set exactly one source"))
            .with_hint(format!(
                "set either `[{field_path}.builtin] language = \"...\" name = \"...\"` or `[{field_path}] path = \"...\"`"
            ));
        diagnostics.push(match job {
            Some(job) => diagnostic.with_job(job.to_owned()),
            None => diagnostic,
        });
    }

    if let BuiltinState::Partial = builtin_state {
        let diagnostic =
            Diagnostic::error(format!("{label} builtin must set both language and name"))
                .with_hint(format!(
                    "set `[{field_path}.builtin] language = \"...\" name = \"...\"`"
                ));
        diagnostics.push(match job {
            Some(job) => diagnostic.with_job(job.to_owned()),
            None => diagnostic,
        });
    } else if let BuiltinState::Complete { language, name } = builtin_state {
        validate_allowed_value(
            diagnostics,
            &format!("{field_path}.builtin.language"),
            language,
            BUILTIN_TEMPLATE_LANGUAGES,
            job,
        );

        let allowed_names = builtin_template_names_for_language(language);
        if !allowed_names.is_empty() {
            validate_allowed_value(
                diagnostics,
                &format!("{field_path}.builtin.name"),
                name,
                allowed_names,
                job,
            );
        }
    }
}

enum BuiltinState<'a> {
    Empty,
    Partial,
    Complete { language: &'a str, name: &'a str },
}

fn validate_allowed_value(
    diagnostics: &mut Vec<Diagnostic>,
    field_name: &str,
    actual: &str,
    allowed: &[&str],
    job: Option<&str>,
) {
    if allowed.contains(&actual) {
        return;
    }

    let message = format!(
        "{field_name} must be one of {} (got `{actual}`)",
        join_allowed_values(allowed)
    );

    let diagnostic = Diagnostic::error(message)
        .with_hint(format!("use one of: {}", join_allowed_values(allowed)));

    diagnostics.push(match job {
        Some(job) => diagnostic.with_job(job.to_owned()),
        None => diagnostic,
    });
}

fn join_allowed_values(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("`{value}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
