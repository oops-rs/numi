use crate::{EntryKind, Metadata, ResourceEntry};
use camino::Utf8PathBuf;
use numi_diagnostics::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEntry {
    pub path: String,
    pub source_path: Utf8PathBuf,
    pub kind: EntryKind,
    pub properties: Metadata,
}

impl RawEntry {
    pub fn leaf(path: impl Into<String>, kind: EntryKind) -> Self {
        Self {
            path: path.into(),
            source_path: Utf8PathBuf::from("fixture"),
            kind,
            properties: Metadata::new(),
        }
    }
}

pub fn swift_identifier(input: &str) -> String {
    let mut identifier = input
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    let mut transformed = String::new();
                    transformed.push(first.to_ascii_uppercase());
                    transformed.push_str(chars.as_str());
                    transformed
                }
                None => String::new(),
            }
        })
        .collect::<String>();

    if identifier.is_empty() {
        identifier.push('_');
    }

    if identifier
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
    {
        identifier.insert(0, '_');
    }

    let keyword_candidate = input.trim().to_ascii_lowercase();
    if is_swift_keyword(&identifier) || is_swift_keyword(&keyword_candidate) {
        format!("`{identifier}`")
    } else {
        identifier
    }
}

pub fn normalize_scope(
    job_name: &str,
    raw_entries: Vec<RawEntry>,
) -> Result<Vec<ResourceEntry>, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut root = NamespaceNode::default();

    for raw in raw_entries {
        root.insert(raw, &mut diagnostics, job_name);
    }

    root.collect_diagnostics(&[], &mut diagnostics, job_name);

    if !diagnostics.is_empty() {
        diagnostics.sort_by(|left, right| {
            left.path
                .as_ref()
                .map(|path| path.as_os_str())
                .cmp(&right.path.as_ref().map(|path| path.as_os_str()))
                .then_with(|| left.message.cmp(&right.message))
        });
        return Err(diagnostics);
    }

    Ok(root.into_root_entries())
}

#[derive(Default)]
struct NamespaceNode {
    namespaces: BTreeMap<String, NamespaceNode>,
    leaves: Vec<RawEntry>,
}

impl NamespaceNode {
    fn insert(&mut self, raw: RawEntry, diagnostics: &mut Vec<Diagnostic>, job_name: &str) {
        let segments = raw
            .path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        if segments.is_empty() {
            diagnostics.push(
                Diagnostic::error(format!("entry in job `{job_name}` has an empty path"))
                    .with_job(job_name),
            );
            return;
        }

        self.insert_segments(raw, &segments, 0);
    }

    fn insert_segments(&mut self, raw: RawEntry, segments: &[String], index: usize) {
        if index + 1 < segments.len() {
            let segment = &segments[index];
            self.namespaces
                .entry(segment.clone())
                .or_default()
                .insert_segments(raw, segments, index + 1);
        } else {
            self.leaves.push(raw);
        }
    }

    fn collect_diagnostics(
        &self,
        scope: &[String],
        diagnostics: &mut Vec<Diagnostic>,
        job_name: &str,
    ) {
        let scope_display = if scope.is_empty() {
            "<root>".to_string()
        } else {
            scope.join("/")
        };
        let mut children = self.immediate_children(scope);
        children.sort_by(|left, right| left.path.cmp(&right.path));

        let mut seen = BTreeMap::<String, ScopeChild>::new();
        for child in children {
            let swift_identifier = swift_identifier(&child.name);
            if let Some(first) = seen.get(&swift_identifier) {
                diagnostics.push(
                    Diagnostic::error(format!(
                        "identifier collision in job `{job_name}` within scope `{}`: `{}` ({}) and `{}` ({}) both normalize to `{}`",
                        scope_display,
                        first.name,
                        first.path,
                        child.name,
                        child.path,
                        swift_identifier
                    ))
                    .with_job(job_name)
                    .with_path(&child.path),
                );
            } else {
                seen.insert(swift_identifier, child);
            }
        }

        for (name, namespace) in &self.namespaces {
            let mut next_scope = scope.to_vec();
            next_scope.push(name.clone());
            namespace.collect_diagnostics(&next_scope, diagnostics, job_name);
        }
    }

    fn immediate_children(&self, scope: &[String]) -> Vec<ScopeChild> {
        let mut children = Vec::with_capacity(self.namespaces.len() + self.leaves.len());
        let scope_prefix = if scope.is_empty() {
            String::new()
        } else {
            scope.join("/")
        };

        for name in self.namespaces.keys() {
            let path = if scope_prefix.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", scope_prefix, name)
            };
            children.push(ScopeChild {
                name: name.clone(),
                path,
            });
        }

        for leaf in &self.leaves {
            let name = leaf
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&leaf.path)
                .to_string();
            children.push(ScopeChild {
                name,
                path: leaf.path.clone(),
            });
        }

        children
    }

    fn into_entries(self, scope: &[String]) -> Vec<ResourceEntry> {
        let mut entries = Vec::new();
        let mut namespace_prefix = scope.to_vec();

        for (name, namespace) in self.namespaces {
            namespace_prefix.push(name.clone());
            entries.push(namespace.into_namespace_entry(&namespace_prefix, name));
            namespace_prefix.pop();
        }

        entries.extend(self.leaves.into_iter().map(|raw| raw.into_entry()));
        entries.sort_by(|left, right| left.id.cmp(&right.id));
        entries
    }

    fn into_namespace_entry(self, scope: &[String], name: String) -> ResourceEntry {
        let mut children = self.into_entries(scope);
        children.sort_by(|left, right| left.id.cmp(&right.id));
        let id = scope.join("/");

        ResourceEntry {
            id,
            name: name.clone(),
            source_path: Utf8PathBuf::from("virtual"),
            swift_identifier: swift_identifier(&name),
            kind: EntryKind::Namespace,
            children,
            properties: Metadata::new(),
            metadata: Metadata::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScopeChild {
    name: String,
    path: String,
}

impl RawEntry {
    fn into_entry(self) -> ResourceEntry {
        let name = self
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&self.path)
            .to_string();
        ResourceEntry {
            id: self.path,
            name: name.clone(),
            source_path: self.source_path,
            swift_identifier: swift_identifier(&name),
            kind: self.kind,
            children: Vec::new(),
            properties: self.properties,
            metadata: Metadata::new(),
        }
    }
}

impl NamespaceNode {
    fn into_root_entries(self) -> Vec<ResourceEntry> {
        self.into_entries(&[])
    }
}

fn is_swift_keyword(identifier: &str) -> bool {
    matches!(
        identifier,
        "associatedtype"
            | "class"
            | "deinit"
            | "enum"
            | "extension"
            | "fileprivate"
            | "func"
            | "import"
            | "init"
            | "inout"
            | "internal"
            | "let"
            | "open"
            | "operator"
            | "private"
            | "protocol"
            | "public"
            | "rethrows"
            | "static"
            | "struct"
            | "subscript"
            | "typealias"
            | "var"
            | "break"
            | "case"
            | "continue"
            | "default"
            | "defer"
            | "do"
            | "else"
            | "fallthrough"
            | "for"
            | "guard"
            | "if"
            | "in"
            | "repeat"
            | "return"
            | "switch"
            | "where"
            | "while"
            | "as"
            | "Any"
            | "catch"
            | "false"
            | "is"
            | "nil"
            | "super"
            | "self"
            | "Self"
            | "throw"
            | "throws"
            | "true"
            | "try"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_normalization_is_stable() {
        assert_eq!(swift_identifier("Icons add"), swift_identifier("Icons-add"));
    }

    #[test]
    fn swift_keywords_are_escaped() {
        assert_eq!(swift_identifier("class"), "`Class`");
        assert_eq!(swift_identifier("Class"), "`Class`");
        assert_eq!(swift_identifier("Self"), "`Self`");
        assert_eq!(swift_identifier("Any"), "`Any`");
    }

    #[test]
    fn collisions_are_reported_within_the_same_namespace_scope() {
        let entries = vec![
            RawEntry::leaf("Icons/add", EntryKind::Image),
            RawEntry::leaf("Icons/Add", EntryKind::Image),
        ];

        let diagnostics = normalize_scope("assets", entries).unwrap_err();
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("collision"));
        assert!(diagnostics[0].message.contains("Icons/add"));
        assert!(diagnostics[0].message.contains("Icons/Add"));
    }

    #[test]
    fn sibling_namespaces_collide_within_the_same_parent_scope() {
        let entries = vec![
            RawEntry::leaf("Icons/Set/add", EntryKind::Image),
            RawEntry::leaf("Icons/set/add", EntryKind::Image),
        ];

        let diagnostics = normalize_scope("assets", entries).unwrap_err();
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("collision"));
    }

    #[test]
    fn normalization_output_is_deterministic() {
        let entries = vec![
            RawEntry::leaf("b/set", EntryKind::Image),
            RawEntry::leaf("a/set", EntryKind::Image),
            RawEntry::leaf("b/alpha", EntryKind::Image),
        ];

        let normalized = normalize_scope("assets", entries).unwrap();
        let ids = normalized
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["a", "b"]);
        assert_eq!(
            normalized[0]
                .children
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a/set"]
        );
        assert_eq!(
            normalized[1]
                .children
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b/alpha", "b/set"]
        );
    }

    #[test]
    fn nested_namespace_ids_preserve_full_path_identity() {
        let normalized = normalize_scope(
            "assets",
            vec![RawEntry::leaf("Icons/Common/add", EntryKind::Image)],
        )
        .unwrap();

        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].id, "Icons");
        assert_eq!(normalized[0].children.len(), 1);
        assert_eq!(normalized[0].children[0].id, "Icons/Common");
        assert_eq!(normalized[0].children[0].children.len(), 1);
        assert_eq!(normalized[0].children[0].children[0].id, "Icons/Common/add");
    }
}
