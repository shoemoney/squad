//! Backward planning: deterministic freeze, lens deal and dependency arithmetic.
use squad_protocol::bass_ackwards_prd::identity;
use squad_protocol::bass_ackwards_prd::*;
use squad_protocol::*;
use std::collections::{BTreeMap, BTreeSet};
pub fn prepare(_parsed: &ParsedInput) -> Result<(), crate::prompts::PromptError> {
    Ok(())
}
pub fn calls(m: u32, human: bool) -> (u32, u32) {
    (
        4 * m + 4 + u32::from(human),
        8 * m + 12 + 3 * u32::from(human),
    )
}
pub fn freeze(
    grouping: &BassAckwardsPrdGrouping,
    items: &serde_json::Value,
) -> BassAckwardsPrdDone {
    let items = items.as_array().expect("validated proposals");
    let mut qualified = vec![];
    let mut single_source = vec![];
    for (i, g) in grouping.groups.iter().enumerate() {
        let sources = items
            .iter()
            .filter(|x| g.source_refs.iter().any(|r| x["ref"] == *r))
            .collect::<Vec<_>>();
        let support = sources
            .iter()
            .filter_map(|x| x["label"].as_str())
            .collect::<BTreeSet<_>>()
            .len();
        if support >= 2 {
            qualified.push((
                i,
                support,
                g.clone(),
                sources
                    .iter()
                    .find(|x| x["test"] == g.text)
                    .map(|x| x["verifiedBy"].as_str().unwrap_or("").to_owned())
                    .unwrap_or_default(),
            ));
        } else {
            single_source.push(g.clone());
        }
    }
    qualified.sort_by_key(|(i, s, _, _)| (std::cmp::Reverse(*s), *i));
    qualified.truncate(12);
    BassAckwardsPrdDone {
        items: qualified
            .into_iter()
            .enumerate()
            .map(
                |(i, (_, support, g, verified_by))| BassAckwardsPrdDoneEntry {
                    id: format!("D{}", i + 1),
                    test: g.text,
                    verified_by,
                    support,
                    source_refs: g.source_refs,
                },
            )
            .collect(),
        single_source,
    }
}
pub fn lens(rotation: &[MemberNick], nick: &MemberNick) -> &'static str {
    ["GAPS", "ATTACK", "READBACK"][rotation
        .iter()
        .position(|n| n == nick)
        .expect("participant")
        % 3]
}
pub fn readback(rotation: &[MemberNick], nick: &MemberNick, milestones: usize) -> usize {
    rotation
        .iter()
        .position(|n| n == nick)
        .expect("participant")
        / 3
        % milestones
}
pub fn mint_keys(d: &mut BassAckwardsPrdDocument) {
    let mut counter = d
        .order()
        .iter()
        .filter_map(|k| k.strip_prefix('K').and_then(|k| k.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    mint_keys_after(d, &mut counter);
}
/// The counter is persisted with every version, including after removals. Keys
/// are stable identities, independent of positional M/P/L display numbers.
pub fn mint_keys_after(d: &mut BassAckwardsPrdDocument, counter: &mut u64) {
    let mut used = d.order().into_iter().collect::<BTreeSet<_>>();
    let mut next_key = || loop {
        *counter += 1;
        let key = format!("K{counter}");
        if used.insert(key.clone()) {
            break key;
        }
    };
    let mut refs = BTreeMap::new();
    for m in &mut d.milestones {
        if m.key.is_none() {
            let k = next_key();
            refs.insert(m.local_id.clone(), k.clone());
            m.key = Some(k);
        }
        for p in &mut m.phases {
            if p.key.is_none() {
                let k = next_key();
                refs.insert(p.local_id.clone(), k.clone());
                p.key = Some(k);
            }
            for l in &mut p.line_items {
                if l.key.is_none() {
                    let k = next_key();
                    refs.insert(l.local_id.clone(), k.clone());
                    l.key = Some(k);
                }
            }
        }
    }
    for m in &mut d.milestones {
        for p in &mut m.phases {
            for l in &mut p.line_items {
                for dep in &mut l.depends_on {
                    if let Some(k) = refs.get(dep) {
                        *dep = k.clone();
                    }
                }
            }
        }
    }
}
pub fn checks(d: &BassAckwardsPrdDocument, done: &BassAckwardsPrdDone) -> BassAckwardsPrdChecks {
    let lines = d.lines();
    let ids = lines
        .iter()
        .map(|l| identity(&l.key, &l.local_id).to_owned())
        .collect::<Vec<_>>();
    let served = lines
        .iter()
        .flat_map(|l| l.serves.iter())
        .collect::<BTreeSet<_>>();
    let uncovered = done
        .items
        .iter()
        .filter(|i| !served.contains(&i.id))
        .map(|i| i.id.clone())
        .collect::<Vec<_>>();
    let dependencies = lines
        .iter()
        .all(|l| l.depends_on.iter().all(|d| ids.contains(d)));
    let mut paths: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for _ in 0..lines.len() {
        for l in &lines {
            let id = identity(&l.key, &l.local_id);
            if paths.contains_key(id) || !l.depends_on.iter().all(|d| paths.contains_key(d)) {
                continue;
            }
            let mut best = vec![];
            for candidate in &ids {
                if l.depends_on.contains(candidate) {
                    let p = &paths[candidate];
                    if p.len() > best.len()
                        || (p.len() == best.len() && path_order(p, &ids) < path_order(&best, &ids))
                    {
                        best = p.clone();
                    }
                }
            }
            best.push(id.into());
            paths.insert(id.into(), best);
        }
    }
    let acyclic = paths.len() == lines.len();
    let mut critical = vec![];
    let mut levels: Vec<Vec<String>> = vec![];
    for id in &ids {
        if let Some(p) = paths.get(id) {
            while levels.len() < p.len() {
                levels.push(vec![]);
            }
            levels[p.len() - 1].push(id.clone());
            if p.len() > critical.len()
                || (p.len() == critical.len() && path_order(p, &ids) < path_order(&critical, &ids))
            {
                critical = p.clone();
            }
        }
    }
    BassAckwardsPrdChecks {
        mapped: lines.iter().all(|l| {
            !l.serves.is_empty()
                && l.serves
                    .iter()
                    .all(|id| done.items.iter().any(|d| &d.id == id))
        }),
        covered: uncovered.is_empty(),
        dependencies,
        acyclic,
        uncovered,
        levels,
        critical_path: critical,
    }
}
fn path_order(path: &[String], ids: &[String]) -> Vec<usize> {
    path.iter()
        .map(|p| ids.iter().position(|i| i == p).unwrap_or(usize::MAX))
        .collect()
}
pub fn slug(title: &str) -> String {
    let mut s = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    s = s.trim_matches('-').chars().take(48).collect();
    if s.is_empty() { "prd".into() } else { s }
}
pub fn blind(value: &mut serde_json::Value, setup: &BassAckwardsPrdSetup) {
    match value {
        serde_json::Value::String(s) => {
            let mut tokens = setup.redaction_tokens.clone();
            tokens.extend(setup.participants.iter().map(ToString::to_string));
            let slugs = tokens
                .iter()
                .filter(|t| t.contains('/'))
                .cloned()
                .collect::<Vec<_>>();
            for slug in slugs {
                tokens.extend(
                    slug.split(['/', '-', '_', '.', '~'])
                        .filter(|p| p.chars().any(char::is_alphabetic))
                        .map(str::to_owned),
                );
            }
            tokens.sort_by_key(|s| std::cmp::Reverse(s.len()));
            for t in tokens {
                if !t.trim().is_empty() {
                    let pattern = format!(r"(?i)\b{}\b", regex::escape(t.trim_start_matches('~')));
                    *s = regex::Regex::new(&pattern)
                        .expect("escaped")
                        .replace_all(s, "[redacted]")
                        .into_owned();
                }
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                blind(v, setup)
            }
        }
        serde_json::Value::Object(o) => {
            for (k, v) in o {
                if ![
                    "key",
                    "localId",
                    "nodeId",
                    "ref",
                    "sourceRefs",
                    "sourceLabels",
                    "sources",
                    "serves",
                    "dependsOn",
                    "under",
                    "parent",
                    "id",
                    "label",
                    "lens",
                ]
                .contains(&k.as_str())
                {
                    blind(v, setup);
                }
            }
        }
        _ => {}
    }
}
pub fn drive(
    router: &mut crate::router::Router,
) -> squad_store::Result<Vec<squad_store::commit::CommitOutcome>> {
    let ids = router.store.call(|db| {
        let mut q = db.prepare(
            "SELECT contest_id FROM contests WHERE active=1 AND kind='bass_ackwards_prd'",
        )?;
        Ok(q.query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    let mut outcomes = vec![];
    for raw in ids {
        let contest = raw
            .parse()
            .map_err(|_| squad_store::bass_ackwards_prd::invalid())?;
        if router
            .engine_scope
            .as_ref()
            .is_some_and(|s| !s.contains(&contest))
        {
            continue;
        }
        for _ in 0..64 {
            let state = router
                .store
                .call(move |db| squad_store::bass_ackwards_prd::state(db, contest))?;
            if state.state.is_terminal() || state.state == BassAckwardsPrdStatus::AwaitingHuman {
                break;
            }
            let id = EventId::new();
            let saving = state.state == BassAckwardsPrdStatus::Saving;
            let outcome = if saving {
                crate::runtime::bass_ackwards_prd::save_file(router, contest)?
            } else {
                router.store.commit(
                    id,
                    "contest-engine".into(),
                    squad_store::commit::digest(&state)?,
                    move |tx, g| crate::router::bass_ackwards_prd::drive_move(tx, g, id, state),
                )?
            };
            let empty = outcome.events.is_empty();
            outcomes.push(outcome);
            if empty {
                break;
            }
        }
    }
    Ok(outcomes)
}

/// Blind projections can redact prose. Restore the original bytes of every
/// unaddressed entity before committing a judge's next version.
pub fn restore_unaddressed(
    previous: &BassAckwardsPrdDocument,
    projected: &BassAckwardsPrdDocument,
    next: &mut BassAckwardsPrdDocument,
    touched: &[String],
    add_only: bool,
) {
    if next.title == projected.title {
        next.title = previous.title.clone();
    }
    if next.overview == projected.overview {
        next.overview = previous.overview.clone();
    }
    if next.overview_provenance == projected.overview_provenance {
        next.overview_provenance = previous.overview_provenance.clone();
    }
    if next.open_questions == projected.open_questions {
        next.open_questions = previous.open_questions.clone();
    }
    for m in &mut next.milestones {
        if let Some(old) = previous
            .milestones
            .iter()
            .find(|old| old.key == m.key && m.key.is_some())
            && (add_only || !touched.iter().any(|k| Some(k) == m.key.as_ref()))
        {
            m.local_id = old.local_id.clone();
            m.title = old.title.clone();
            m.goal = old.goal.clone();
            m.provenance = old.provenance.clone();
        }
        for p in &mut m.phases {
            if let Some(old) = previous
                .milestones
                .iter()
                .flat_map(|m| &m.phases)
                .find(|old| old.key == p.key && p.key.is_some())
                && (add_only || !touched.iter().any(|k| Some(k) == p.key.as_ref()))
            {
                p.local_id = old.local_id.clone();
                p.title = old.title.clone();
            }
            for l in &mut p.line_items {
                if let Some(old) = previous
                    .lines()
                    .into_iter()
                    .find(|old| old.key == l.key && l.key.is_some())
                    && (add_only || !touched.iter().any(|k| Some(k) == l.key.as_ref()))
                {
                    let dependencies = l.depends_on.clone();
                    *l = old.clone();
                    if add_only {
                        l.depends_on = dependencies;
                    }
                }
            }
        }
    }
}
