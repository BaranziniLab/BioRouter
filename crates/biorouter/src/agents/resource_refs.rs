#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeBaseRef {
    pub id: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResourceRefs {
    pub skills: Vec<String>,
    pub extensions: Vec<String>,
    pub knowledge_bases: Vec<KnowledgeBaseRef>,
}

impl ResourceRefs {
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty() && self.extensions.is_empty() && self.knowledge_bases.is_empty()
    }
}

pub(crate) fn extract_resource_refs(text: &str) -> ResourceRefs {
    let mut refs = ResourceRefs::default();

    extract_tag_refs(text, "skill", "name", &mut refs.skills);
    extract_tag_refs(text, "extension", "name", &mut refs.extensions);
    extract_kb_tag_refs(text, &mut refs.knowledge_bases);

    extract_legacy_resource_phrases(text, "skill", &mut refs.skills);
    extract_legacy_resource_phrases(text, "extension", &mut refs.extensions);
    extract_inline_refs(text, "skill:", &mut refs.skills);
    extract_inline_refs(text, "ext:", &mut refs.extensions);
    extract_inline_kb_refs(text, &mut refs.knowledge_bases);
    extract_function_refs(text, "/skill(", ")", &mut refs.skills);
    extract_function_refs(text, "/ext(", ")", &mut refs.extensions);
    extract_function_kb_refs(text, &mut refs.knowledge_bases);
    extract_legacy_kb_refs(text, &mut refs.knowledge_bases);

    dedup(&mut refs.skills);
    dedup(&mut refs.extensions);
    dedup_kbs(&mut refs.knowledge_bases);

    refs
}

pub(crate) fn canonical_builtin_extension_name(name: &str) -> Option<String> {
    let normalized = normalize_reference_name(name);
    let aliases = [
        ("agentdrafter", "agent_drafter"),
        ("agent-drafter", "agent_drafter"),
        ("agent_drafter", "agent_drafter"),
        ("extensionmanager", "Extension Manager"),
        ("extension-manager", "Extension Manager"),
        ("extension_manager", "Extension Manager"),
        ("autovisualizer", "autovisualiser"),
        ("auto-visualizer", "autovisualiser"),
        ("auto_visualizer", "autovisualiser"),
        ("autovisualiser", "autovisualiser"),
        ("auto-visualiser", "autovisualiser"),
        ("auto_visualiser", "autovisualiser"),
    ];

    if let Some((_, canonical)) = aliases
        .iter()
        .find(|(alias, _)| normalize_reference_name(alias) == normalized)
    {
        return Some((*canonical).to_string());
    }

    biorouter_mcp::BUILTIN_EXTENSIONS
        .keys()
        .find(|key| normalize_reference_name(key) == normalized)
        .map(|key| (*key).to_string())
}

fn normalize_reference_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn extract_tag_refs(text: &str, tag_type: &str, attr: &str, out: &mut Vec<String>) {
    let needle = format!("<biorouter-ref type=\"{tag_type}\" {attr}=\"");
    let mut rest = text;
    while let Some(start) = rest.find(&needle) {
        let value_start = start + needle.len();
        let Some(end) = rest[value_start..].find('"') else {
            break;
        };
        push_trimmed(out, &rest[value_start..value_start + end]);
        rest = &rest[value_start + end..];
    }
}

fn extract_kb_tag_refs(text: &str, out: &mut Vec<KnowledgeBaseRef>) {
    let needle = "<biorouter-ref type=\"knowledge_base\" id=\"";
    let mut rest = text;
    while let Some(start) = rest.find(needle) {
        let id_start = start + needle.len();
        let Some(id_end) = rest[id_start..].find('"') else {
            break;
        };
        let id = rest[id_start..id_start + id_end].trim();
        if !id.is_empty() {
            out.push(KnowledgeBaseRef {
                id: id.to_string(),
                label: None,
            });
        }
        rest = &rest[id_start + id_end..];
    }
}

fn extract_function_refs(text: &str, prefix: &str, suffix: &str, out: &mut Vec<String>) {
    let mut rest = text;
    while let Some(start) = rest.find(prefix) {
        let value_start = start + prefix.len();
        let Some(end) = rest[value_start..].find(suffix) else {
            break;
        };
        push_trimmed(out, &rest[value_start..value_start + end]);
        rest = &rest[value_start + end + suffix.len()..];
    }
}

fn extract_function_kb_refs(text: &str, out: &mut Vec<KnowledgeBaseRef>) {
    let mut refs = Vec::new();
    extract_function_refs(text, "/kb(", ")", &mut refs);
    for id in refs {
        out.push(KnowledgeBaseRef { id, label: None });
    }
}

fn extract_legacy_resource_phrases(text: &str, kind: &str, out: &mut Vec<String>) {
    let marker = format!("\" {kind}");
    let mut rest = text;
    while let Some(marker_start) = rest.find(&marker) {
        let before_marker = &rest[..marker_start];
        let Some(open_quote) = before_marker.rfind('"') else {
            rest = &rest[marker_start + marker.len()..];
            continue;
        };
        push_trimmed(out, &before_marker[open_quote + 1..]);
        rest = &rest[marker_start + marker.len()..];
    }
}

fn extract_inline_refs(text: &str, prefix: &str, out: &mut Vec<String>) {
    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| c == ',' || c == '.' || c == ';');
        let value = trimmed.strip_prefix(&format!("/{prefix}"));
        if let Some(value) = value {
            push_trimmed(out, value);
        }
    }
}

fn extract_inline_kb_refs(text: &str, out: &mut Vec<KnowledgeBaseRef>) {
    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| c == ',' || c == '.' || c == ';');
        let value = trimmed.strip_prefix("/kb:");
        if let Some(value) = value {
            let id = value.trim();
            if !id.is_empty() {
                out.push(KnowledgeBaseRef {
                    id: id.to_string(),
                    label: None,
                });
            }
        }
    }
}

fn extract_legacy_kb_refs(text: &str, out: &mut Vec<KnowledgeBaseRef>) {
    let mut rest = text;
    while let Some(kb_start) = rest.find("kb_id:") {
        let after = rest[kb_start + "kb_id:".len()..].trim_start();
        let after = after.strip_prefix('"').unwrap_or(after);
        let after = after.strip_prefix('`').unwrap_or(after);
        let id_len = after
            .find(|c: char| c == ')' || c == '"' || c == '`' || c == ',' || c.is_whitespace())
            .unwrap_or(after.len());
        let id = after[..id_len].trim();
        if !id.is_empty() {
            out.push(KnowledgeBaseRef {
                id: id.to_string(),
                label: None,
            });
        }
        rest = &after[id_len..];
    }

    let mut phrase_rest = text;
    while let Some(start) = phrase_rest.find("focus the \"") {
        let value_start = start + "focus the \"".len();
        let Some(end) = phrase_rest[value_start..].find("\" knowledge base") else {
            break;
        };
        let id = phrase_rest[value_start..value_start + end].trim();
        if !id.is_empty() {
            out.push(KnowledgeBaseRef {
                id: id.to_string(),
                label: None,
            });
        }
        phrase_rest = &phrase_rest[value_start + end..];
    }
}

fn push_trimmed(out: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        out.push(value.to_string());
    }
}

fn dedup(values: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn dedup_kbs(values: &mut Vec<KnowledgeBaseRef>) {
    let mut seen = std::collections::HashSet::new();
    values.retain(|value| seen.insert(value.id.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_machine_readable_refs() {
        let refs = extract_resource_refs(
            r#"<biorouter-ref type="skill" name="rna-qc"> <biorouter-ref type="extension" name="developer"> <biorouter-ref type="knowledge_base" id="soul">"#,
        );

        assert_eq!(refs.skills, vec!["rna-qc"]);
        assert_eq!(refs.extensions, vec!["developer"]);
        assert_eq!(
            refs.knowledge_bases,
            vec![KnowledgeBaseRef {
                id: "soul".to_string(),
                label: None
            }]
        );
    }

    #[test]
    fn extracts_legacy_inserted_phrases() {
        let refs = extract_resource_refs(
            r#"Use the "literature-review" skill for this request, Use the "pubmed" extension for this request, Using the Knowledge extension, focus knowledge base "Soul" (kb_id: soul) for this request"#,
        );

        assert_eq!(refs.skills, vec!["literature-review"]);
        assert_eq!(refs.extensions, vec!["pubmed"]);
        assert_eq!(refs.knowledge_bases[0].id, "soul");
    }

    #[test]
    fn extracts_inline_refs() {
        let refs = extract_resource_refs(
            "/skill:rna-qc /ext:developer /kb:soul /skill(rna-qc-v2) /ext(agentdrafter) /kb(project-notes)",
        );

        assert_eq!(refs.skills, vec!["rna-qc", "rna-qc-v2"]);
        assert_eq!(refs.extensions, vec!["developer", "agentdrafter"]);
        assert_eq!(refs.knowledge_bases[0].id, "soul");
        assert_eq!(refs.knowledge_bases[1].id, "project-notes");
    }

    #[test]
    fn recognizes_builtin_extension_aliases() {
        assert_eq!(
            canonical_builtin_extension_name("agentdrafter"),
            Some("agent_drafter".to_string())
        );
        assert_eq!(
            canonical_builtin_extension_name("autovisualizer"),
            Some("autovisualiser".to_string())
        );
    }
}
