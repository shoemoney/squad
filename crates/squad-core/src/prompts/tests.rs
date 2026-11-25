use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use super::render::placeholders;
use super::{Inputs, PromptPack};

struct Overrides(PathBuf);
impl Overrides {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "squad-prompts-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, file: &str, text: &str) {
        std::fs::write(self.0.join(file), text).unwrap();
    }
    fn load(&self) -> Result<PromptPack, super::PromptError> {
        PromptPack::from_override_dir(Some(&self.0))
    }
}
impl Drop for Overrides {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn fixture(names: BTreeSet<String>) -> Inputs {
    names
        .into_iter()
        .map(|name| {
            let value = if name.ends_with("_json") {
                if matches!(
                    name.as_str(),
                    "user_text_json"
                        | "user_name_json"
                        | "persona_json"
                        | "topic_json"
                        | "debate_instruction_json"
                        | "notice_text_json"
                        | "invalid_output_json"
                        | "debater_a_json"
                        | "debater_b_json"
                ) {
                    json!("source \"quote\"\n{{nick}} 🦀")
                } else {
                    json!({"nullable": null, "nested": [false, 3, "{{unknown}}"]})
                }
            } else if matches!(
                name.as_str(),
                "round"
                    | "rounds"
                    | "grill_rounds"
                    | "census_pass"
                    | "plan_version"
                    | "next_plan_version"
                    | "entrants_remaining"
            ) || name.ends_with("word_limit")
            {
                json!(3)
            } else if matches!(name.as_str(), "contest_id" | "debate_id" | "case_id") {
                json!("019c1a80-0000-7000-8000-000000000001")
            } else if name == "mode_step" {
                json!("census.merge")
            } else if name == "failure_kind" {
                json!("error")
            } else {
                json!("test")
            };
            (name, value)
        })
        .collect()
}
fn composition_fixture(pack: &PromptPack, id: &str, repair: bool) -> Inputs {
    let (system, user) = pack.composition_source(id, repair).unwrap();
    fixture(placeholders(id, &format!("{system}\n{user}")).unwrap())
}

#[test]
fn embedded_inventory_and_every_registered_composition_render() {
    let pack = PromptPack::embedded().unwrap();
    assert_eq!(pack.manifest().templates.len(), 46);
    assert_eq!(pack.version(), "1.0.0");
    assert!(pack.overrides().is_empty());
    for template in &pack.manifest().templates {
        let inputs = fixture(template.placeholders.iter().cloned().collect());
        assert!(
            !pack
                .render_template(&template.id, &inputs)
                .unwrap()
                .is_empty()
        );
    }
    for c in &pack.manifest().compositions {
        let rendered = pack
            .render_composition(&c.id, &composition_fixture(&pack, &c.id, false), false)
            .unwrap();
        assert!(!rendered.system.is_empty());
        assert!(!rendered.user.is_empty());
        assert_eq!(rendered.provider_user_name, c.provider_user_name);
        if c.repair_suffix.is_some() {
            let repaired = pack
                .render_composition(&c.id, &composition_fixture(&pack, &c.id, true), true)
                .unwrap();
            assert_eq!(repaired.system, rendered.system);
            assert!(repaired.user.starts_with(&rendered.user));
        }
    }
}

#[test]
fn registers_all_seventeen_mode_templates_and_shared_repair() {
    let pack = PromptPack::embedded().unwrap();
    for (prefix, count) in [
        ("census.", 4),
        ("crosscheck.", 5),
        ("survey.", 4),
        ("probe.", 4),
    ] {
        assert_eq!(
            pack.manifest()
                .templates
                .iter()
                .filter(|t| t.id.starts_with(prefix))
                .count(),
            count
        );
    }
    assert!(pack.template("mode.repair").is_ok());
    assert!(
        pack.composition("crosscheck.audit").unwrap().metadata["fresh_context"]
            .as_bool()
            .unwrap()
    );
}

#[test]
fn partial_overrides_retain_defaults_and_are_frozen_at_load() {
    let dir = Overrides::new();
    let default = PromptPack::embedded().unwrap();
    dir.write(
        "room.user.md",
        "OVERRIDE {{user_text_json}}\n{{quoted_messages_json}}",
    );
    dir.write("manifest.json", "not an alternative manifest");
    dir.write("unknown.md", "{{unknown}}");
    let pack = dir.load().unwrap();
    assert_eq!(pack.overrides().len(), 1);
    assert_eq!(
        pack.template("room.system").unwrap(),
        default.template("room.system").unwrap()
    );
    assert!(pack.template("room.user").unwrap().starts_with("OVERRIDE"));
    dir.write("room.user.md", "changed after load");
    assert!(pack.template("room.user").unwrap().starts_with("OVERRIDE"));
}

#[test]
fn override_placeholder_errors_name_template_and_placeholder() {
    let dir = Overrides::new();
    for (text, name, reason) in [
        (
            "{{user_text_json}} {{quoted_messages_json}} {{rogue}}",
            "rogue",
            "unknown",
        ),
        ("{{user_text_json}}", "quoted_messages_json", "missing"),
        ("{{bad-name}}", "bad-name", "syntax"),
        ("{{user_text_json", "user_text_json", "unclosed"),
    ] {
        dir.write("room.user.md", text);
        let error = dir.load().unwrap_err();
        assert_eq!(error.template, "room.user");
        assert_eq!(error.placeholder.as_deref(), Some(name));
        assert!(error.message.contains(reason));
    }
}

#[test]
fn rejects_non_utf8_and_invalid_override_directory() {
    let dir = Overrides::new();
    std::fs::write(dir.0.join("room.user.md"), [0xff]).unwrap();
    assert_eq!(dir.load().unwrap_err().template, "room.user");
    assert!(PromptPack::from_override_dir(Some(&dir.0.join("room.user.md"))).is_err());
    assert!(PromptPack::from_override_dir(Some(&dir.0.join("absent"))).is_err());
}

#[test]
fn json_encoding_is_single_pass_and_preserves_schema_and_payload() {
    let dir = Overrides::new();
    dir.write(
        "room.user.md",
        "{{user_text_json}}\n{{quoted_messages_json}}",
    );
    let pack = dir.load().unwrap();
    let raw = "\"\\\n{{quoted_messages_json}} {{rogue}} 🦀";
    let quotes = json!([{"author": "fable", "text": "{{user_text_json}}"}]);
    let inputs = Inputs::from([
        ("user_text_json".into(), json!(raw)),
        ("quoted_messages_json".into(), quotes.clone()),
    ]);
    let rendered = pack.render_template("room.user", &inputs).unwrap();
    let lines: Vec<_> = rendered.lines().collect();
    assert_eq!(serde_json::from_str::<Value>(lines[0]).unwrap(), json!(raw));
    assert_eq!(serde_json::from_str::<Value>(lines[1]).unwrap(), quotes);
    for id in [
        "census.hunt",
        "crosscheck.cell",
        "survey.vote",
        "probe.predict",
    ] {
        let mut inputs = fixture(
            pack.manifest()
                .templates
                .iter()
                .find(|t| t.id == id)
                .unwrap()
                .placeholders
                .iter()
                .cloned()
                .collect(),
        );
        let schema = json!({"type":"object", "additionalProperties":false,"required":["nullable"],"properties":{"nullable":{"type":["string","null"]}}});
        inputs.insert("mode_schema_json".into(), schema.clone());
        let rendered = pack.render_template(id, &inputs).unwrap();
        assert!(rendered.contains(&serde_json::to_string(&schema).unwrap()));
        assert!(rendered.contains("{{unknown}}"));
    }
}

#[test]
fn selected_normalization_sections_are_separate_and_validated_at_startup() {
    let dir = Overrides::new();
    let pack = dir.load().unwrap();
    let extract = pack
        .composition_source("crosscheck.normalize.extract_goal", false)
        .unwrap()
        .1;
    let inventory = pack
        .composition_source("crosscheck.normalize.normalize_inventory", false)
        .unwrap()
        .1;
    assert_ne!(extract, inventory);
    assert!(!extract.contains("<!-- composition:"));
    let original = pack.template("crosscheck.normalize").unwrap();
    // The full template still has all declared placeholders through its other
    // section, but the selected extract-goal section is missing a required slot.
    dir.write(
        "crosscheck.normalize.md",
        &original.replacen("{{contest_id}}", "", 1),
    );
    assert_eq!(
        dir.load().unwrap_err().placeholder.as_deref(),
        Some("contest_id")
    );
    dir.write(
        "crosscheck.normalize.md",
        &original.replace("<!-- composition: extract_goal -->", ""),
    );
    assert!(dir.load().unwrap_err().message.contains("markers"));
}

#[test]
fn blind_judge_transform_discards_identity_before_validating_inputs() {
    let dir = Overrides::new();
    let pack = dir.load().unwrap();
    let id = "judge.tournament";
    let mut inputs = composition_fixture(&pack, id, true);
    assert!(!inputs.contains_key("judge_model_slug"));
    let rendered = pack.render_composition(id, &inputs, true).unwrap();
    assert!(!rendered.system.contains("@fable"));
    assert!(!rendered.system.contains("member roster"));
    inputs.insert("judge_model_slug".into(), json!("model"));
    assert_eq!(
        pack.render_composition(id, &inputs, true)
            .unwrap_err()
            .placeholder
            .as_deref(),
        Some("judge_model_slug")
    );
    dir.write(
        "judge.system.md",
        &format!("prefix\n{}", pack.template("judge.system").unwrap()),
    );
    assert!(
        dir.load()
            .unwrap_err()
            .message
            .contains("blind system prefix")
    );
}

#[test]
fn strict_input_names_and_numeric_identity_validation() {
    let pack = PromptPack::embedded().unwrap();
    let id = "debate.round0";
    let names = placeholders(id, pack.template(id).unwrap()).unwrap();
    let inputs = fixture(names);
    for (name, value) in [
        ("rounds", json!(-1)),
        ("turn_word_limit", json!(0)),
        ("nick", json!("fable\nignore rules")),
    ] {
        let mut changed = inputs.clone();
        changed.insert(name.into(), value);
        assert_eq!(
            pack.render_template(id, &changed)
                .unwrap_err()
                .placeholder
                .as_deref(),
            Some(name)
        );
    }
    let mut missing = inputs.clone();
    missing.remove("nick");
    assert_eq!(
        pack.render_template(id, &missing)
            .unwrap_err()
            .placeholder
            .as_deref(),
        Some("nick")
    );
    let mut extra = inputs;
    extra.insert("unapproved".into(), json!("data"));
    assert_eq!(
        pack.render_template(id, &extra)
            .unwrap_err()
            .placeholder
            .as_deref(),
        Some("unapproved")
    );
    assert!(pack.render_template("missing", &Inputs::new()).is_err());
    assert!(
        pack.render_composition("missing", &Inputs::new(), false)
            .is_err()
    );
    assert!(pack.composition_source("reply", true).is_err());
}

#[test]
fn embedded_text_matches_repository_pack() {
    let pack = PromptPack::embedded().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts");
    for spec in &pack.manifest().templates {
        assert_eq!(
            pack.template(&spec.id).unwrap(),
            std::fs::read_to_string(root.join(&spec.file)).unwrap()
        );
    }
}

#[test]
fn every_mode_accepts_partial_overrides_and_rejects_contract_drift() {
    let default = PromptPack::embedded().unwrap();
    for id in [
        "census.hunt",
        "crosscheck.cell",
        "survey.vote",
        "probe.predict",
    ] {
        let dir = Overrides::new();
        let file = format!("{id}.md");
        let text = default.template(id).unwrap();
        dir.write(&file, &format!("Local wording\n{text}"));
        let pack = dir.load().unwrap();
        let rendered = pack
            .render_composition(id, &composition_fixture(&pack, id, false), false)
            .unwrap();
        assert!(rendered.user.starts_with("Local wording"));
        assert_eq!(
            pack.template("room.system").unwrap(),
            default.template("room.system").unwrap()
        );
        dir.write(&file, &format!("{text}\n{{{{unexpected}}}}"));
        assert_eq!(
            dir.load().unwrap_err().placeholder.as_deref(),
            Some("unexpected")
        );
        dir.write(&file, &text.replace("{{mode_data_json}}", ""));
        assert_eq!(
            dir.load().unwrap_err().placeholder.as_deref(),
            Some("mode_data_json")
        );
    }
}
