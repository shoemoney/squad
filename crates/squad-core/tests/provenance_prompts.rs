use squad_core::prompts::{PromptPack, provenance::NEW_MODE_COMPOSITIONS};
use std::collections::BTreeSet;

#[test]
fn every_new_mode_composition_is_registered_and_hashed_distinctly() {
    let pack = PromptPack::embedded().unwrap();
    let hashes = pack.composition_hashes().unwrap();
    assert_eq!(hashes.len(), pack.manifest().compositions.len());
    let mut seen = BTreeSet::new();
    for id in NEW_MODE_COMPOSITIONS {
        let hash = hashes
            .get(id)
            .unwrap_or_else(|| panic!("{id} unregistered"));
        assert_eq!(hash.len(), 64, "{id}");
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(seen.insert(hash.clone()), "{id} collides");
        let reference = pack.prompt_ref(id).unwrap();
        assert_eq!(reference.template_id.as_str(), id);
        assert_eq!(reference.pack_version.as_str(), pack.version());
        assert_eq!(reference.template_hash.as_str(), hash);
    }
    assert_eq!(NEW_MODE_COMPOSITIONS.len(), 18);
}

#[test]
fn hashes_are_stable_across_loads_and_follow_effective_override_bytes() {
    let a = PromptPack::embedded()
        .unwrap()
        .composition_hashes()
        .unwrap();
    let b = PromptPack::embedded()
        .unwrap()
        .composition_hashes()
        .unwrap();
    assert_eq!(a, b);
    let dir = tempfile::tempdir().unwrap();
    let original = PromptPack::embedded()
        .unwrap()
        .template("census.merge")
        .unwrap()
        .to_owned();
    std::fs::write(
        dir.path().join("census.merge.md"),
        format!("{original}\nEXTRA\n"),
    )
    .unwrap();
    let overridden = PromptPack::from_override_dir(Some(dir.path())).unwrap();
    let c = overridden.composition_hashes().unwrap();
    assert_ne!(a["census.merge"], c["census.merge"]);
    for (id, hash) in &a {
        if id != "census.merge" {
            assert_eq!(hash, &c[id], "{id} must not move");
        }
    }
}

#[test]
fn repair_variants_change_only_through_the_registered_suffix() {
    let pack = PromptPack::embedded().unwrap();
    for id in NEW_MODE_COMPOSITIONS {
        let spec = pack.composition(id).unwrap();
        let (_, plain) = pack.composition_source(id, false).unwrap();
        let (_, repaired) = pack.composition_source(id, true).unwrap();
        assert_eq!(spec.repair_suffix.is_some(), plain != repaired, "{id}");
    }
}
