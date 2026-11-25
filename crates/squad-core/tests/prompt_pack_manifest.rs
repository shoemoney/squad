use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

fn prompt_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../prompts")
}

fn manifest() -> Value {
    serde_json::from_str(
        &fs::read_to_string(prompt_root().join("manifest.json")).expect("read prompt manifest"),
    )
    .expect("parse prompt manifest")
}

fn placeholders(text: &str) -> BTreeSet<&str> {
    let mut found = BTreeSet::new();
    let mut remaining = text;
    while let Some((_, after)) = remaining.split_once("{{") {
        let (name, rest) = after.split_once("}}").expect("closed placeholder");
        assert!(
            !name.is_empty() && name.bytes().all(|c| c.is_ascii_lowercase() || c == b'_'),
            "invalid placeholder: {name}"
        );
        found.insert(name);
        remaining = rest;
    }
    found
}

fn strings(value: &Value) -> BTreeSet<&str> {
    let entries = value.as_array().expect("array of strings");
    let result: BTreeSet<_> = entries
        .iter()
        .map(|entry| entry.as_str().expect("string entry"))
        .collect();
    assert_eq!(result.len(), entries.len(), "duplicate declaration");
    result
}

#[test]
fn templates_exist_with_unique_ids_and_exact_placeholder_declarations() {
    let manifest = manifest();
    assert!(
        !manifest["version"]
            .as_str()
            .expect("pack version")
            .is_empty()
    );
    let templates = manifest["templates"].as_array().expect("templates");
    assert!(!templates.is_empty());
    let mut ids = BTreeSet::new();
    let mut files = BTreeSet::new();
    for template in templates {
        let id = template["id"].as_str().expect("template ID");
        assert!(ids.insert(id), "duplicate template ID: {id}");
        let file = template["file"].as_str().expect("template file");
        assert!(files.insert(file), "duplicate template file: {file}");
        let text = fs::read_to_string(prompt_root().join(file))
            .unwrap_or_else(|error| panic!("cannot read {file}: {error}"));
        assert_eq!(
            placeholders(&text),
            strings(&template["placeholders"]),
            "placeholder declarations differ for {id}"
        );
    }
    let actual_files: BTreeSet<_> = fs::read_dir(prompt_root())
        .expect("prompt directory")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual_files, files.into_iter().map(str::to_owned).collect());
}

#[test]
fn compositions_reference_registered_templates_and_select_valid_sections() {
    let manifest = manifest();
    let templates = manifest["templates"].as_array().unwrap();
    let registered: BTreeSet<_> = templates
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    let mut ids = BTreeSet::new();
    for composition in manifest["compositions"].as_array().expect("compositions") {
        let id = composition["id"].as_str().expect("composition ID");
        assert!(ids.insert(id), "duplicate composition ID: {id}");
        for field in ["system", "user"] {
            for template in composition[field].as_array().expect("template list") {
                assert!(
                    registered.contains(template.as_str().unwrap()),
                    "{id}: {field}"
                );
            }
        }
        if let Some(repair) = composition["repair_suffix"].as_str() {
            assert!(registered.contains(repair), "unknown repair for {id}");
        }
        if let Some(selection) = composition.get("template_selection") {
            let template = templates
                .iter()
                .find(|t| t["id"] == selection["template"])
                .unwrap();
            let text =
                fs::read_to_string(prompt_root().join(template["file"].as_str().unwrap())).unwrap();
            let start = selection["start_marker"].as_str().unwrap();
            let end = selection["end_marker"].as_str().unwrap();
            assert_eq!(text.matches(start).count(), 1);
            assert_eq!(text.matches(end).count(), 1);
            let selected = text.split_once(start).unwrap().1.split_once(end).unwrap().0;
            assert_eq!(
                placeholders(selected),
                strings(&composition["placeholders"])
            );
        }
        if let Some(transform) = composition.get("system_transform") {
            let template = templates
                .iter()
                .find(|t| t["id"] == transform["template"])
                .unwrap();
            let text =
                fs::read_to_string(prompt_root().join(template["file"].as_str().unwrap())).unwrap();
            let prefix = transform["prefix"].as_str().unwrap();
            assert!(text.starts_with(prefix));
            assert_eq!(text.matches(prefix).count(), 1);
            let replacement = templates
                .iter()
                .find(|t| t["id"] == transform["replacement_template"])
                .unwrap();
            let replacement =
                fs::read_to_string(prompt_root().join(replacement["file"].as_str().unwrap()))
                    .unwrap();
            let blind = format!(
                "{}{}",
                replacement.trim_end(),
                text.strip_prefix(prefix).unwrap()
            );
            assert_eq!(
                placeholders(&blind),
                strings(&composition["system_placeholders"])
            );
            for forbidden in [
                "{{judge_model_slug}}",
                "{{member_nicks_json}}",
                "{{user_name_json}}",
                "@fable",
            ] {
                assert!(
                    !blind.contains(forbidden),
                    "identity in blind system: {forbidden}"
                );
            }
        }
    }
}
