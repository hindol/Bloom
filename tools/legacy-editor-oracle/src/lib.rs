#[cfg(test)]
mod tests {
    use bloom_test_harness::SimInput;

    const ORACLE: &str = include_str!("../../../testdata/adr3_editor_repeat.v1");
    const REPLACEMENT_ORACLE: &str =
        include_str!("../../../testdata/adr3_editor_replace_repeat.v1");

    fn decode(text: &str) -> String {
        text.replace("\\n", "\n").replace("\\r", "\r")
    }

    fn unique_deletion(before: &str, after: &str) -> Vec<(usize, usize)> {
        let before = before.chars().collect::<Vec<_>>();
        let after = after.chars().collect::<Vec<_>>();
        let Some(deleted) = before.len().checked_sub(after.len()) else {
            return Vec::new();
        };
        if deleted == 0 {
            return Vec::new();
        }
        (0..=before.len() - deleted)
            .filter_map(|start| {
                let end = start + deleted;
                let matches = before[..start]
                    .iter()
                    .chain(&before[end..])
                    .copied()
                    .eq(after.iter().copied());
                matches.then_some((start, end))
            })
            .collect()
    }

    fn minimum_replacements(before: &str, after: &str) -> Vec<(usize, usize, String)> {
        let before = before.chars().collect::<Vec<_>>();
        let after = after.chars().collect::<Vec<_>>();
        if before == after {
            return Vec::new();
        }

        let mut minimum = usize::MAX;
        let mut candidates = Vec::new();
        for start in 0..=before.len().min(after.len()) {
            if before[..start] != after[..start] {
                continue;
            }
            for end in start..=before.len() {
                let suffix_len = before.len() - end;
                if start + suffix_len > after.len()
                    || before[end..] != after[after.len() - suffix_len..]
                {
                    continue;
                }
                let replacement_end = after.len() - suffix_len;
                let cost = end - start + replacement_end - start;
                if cost < minimum {
                    minimum = cost;
                    candidates.clear();
                }
                if cost == minimum {
                    candidates.push((start, end, after[start..replacement_end].iter().collect()));
                }
            }
        }
        candidates
    }

    #[test]
    fn deletion_inference_detects_ambiguous_and_non_deletion_changes() {
        assert_eq!(unique_deletion("aaa", "aa"), [(0, 1), (1, 2), (2, 3)]);
        assert!(unique_deletion("abc", "axc").is_empty());
        assert!(unique_deletion("abc", "abcd").is_empty());
        assert_eq!(unique_deletion("a\nb", "ab"), [(1, 2)]);
    }

    #[test]
    fn replacement_inference_detects_ambiguous_and_minimal_changes() {
        assert_eq!(
            minimum_replacements("aaa", "aa"),
            [(0, 1, "".into()), (1, 2, "".into()), (2, 3, "".into())]
        );
        assert_eq!(
            minimum_replacements("aa", "aaa"),
            [(0, 0, "a".into()), (1, 1, "a".into()), (2, 2, "a".into())]
        );
        assert_eq!(minimum_replacements("abc", "axc"), [(1, 2, "x".into())]);
        assert_eq!(minimum_replacements("αβ", "αΩ"), [(1, 2, "Ω".into())]);
        assert!(minimum_replacements("abc", "abc").is_empty());
    }

    #[test]
    fn replacement_inference_matches_exhaustive_reference_model() {
        let mut texts = vec![String::new()];
        let mut frontier = texts.clone();
        for _ in 0..3 {
            frontier = frontier
                .iter()
                .flat_map(|prefix| ['a', 'β'].map(|ch| format!("{prefix}{ch}")))
                .collect();
            texts.extend(frontier.iter().cloned());
        }

        let mut cases = 0;
        for before in &texts {
            let chars = before.chars().collect::<Vec<_>>();
            for after in &texts {
                let mut expected = Vec::new();
                let mut minimum = usize::MAX;
                if before != after {
                    for start in 0..=chars.len() {
                        for end in start..=chars.len() {
                            for replacement in &texts {
                                let candidate = chars[..start]
                                    .iter()
                                    .copied()
                                    .chain(replacement.chars())
                                    .chain(chars[end..].iter().copied())
                                    .collect::<String>();
                                if &candidate != after {
                                    continue;
                                }
                                let cost = end - start + replacement.chars().count();
                                if cost < minimum {
                                    minimum = cost;
                                    expected.clear();
                                }
                                if cost == minimum {
                                    expected.push((start, end, replacement.clone()));
                                }
                            }
                        }
                    }
                }
                assert_eq!(
                    minimum_replacements(before, after),
                    expected,
                    "{before:?} -> {after:?}"
                );
                cases += 1;
            }
        }
        assert_eq!(cases, 225);
    }

    #[test]
    #[ignore = "known frozen legacy bug: Unicode input advances a character cursor by byte length"]
    fn legacy_unicode_insert_preserves_typing_order() {
        let mut failures = Vec::new();
        for raw in ["ab\n", "ab ^edge1\n"] {
            let mut sim = SimInput::with_content(raw);
            sim.keys("i").type_text("Ωλ").keys("<Esc>");
            let actual = sim
                .editor
                .active_buffer_text()
                .expect("active scratch page");
            if actual != "Ωλab\n" {
                failures.push(format!("{raw:?}: {actual:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn editor_repeat_snapshots_match_unambiguous_deletion_ranges() {
        let mut lines = ORACLE.lines();
        assert_eq!(lines.next(), Some("BLOOM_EDITOR_REPEAT_ORACLE_V1"));
        let mut cases = 0;
        for line in lines.filter(|line| !line.is_empty()) {
            let fields = line.split('|').collect::<Vec<_>>();
            assert_eq!(fields.len(), 5, "invalid oracle row: {line}");
            let (name, raw, expected, setup, steps) = (
                fields[0],
                decode(fields[1]),
                decode(fields[2]),
                fields[3],
                fields[4],
            );
            let mut sim = SimInput::with_content(&raw);
            assert_eq!(
                sim.editor
                    .active_buffer_text()
                    .expect("active scratch page"),
                expected,
                "{name}: initial visible text"
            );
            if setup != "-" {
                sim.keys(setup);
            }
            for step in steps.split(';') {
                let fields = step.splitn(4, ',').collect::<Vec<_>>();
                assert_eq!(fields.len(), 4, "{name}: invalid step");
                let expected_after = decode(fields[3]);
                let before = sim
                    .editor
                    .active_buffer_text()
                    .expect("active scratch page");
                sim.keys(fields[0]);
                let after = sim
                    .editor
                    .active_buffer_text()
                    .expect("active scratch page");
                assert_eq!(after, expected_after, "{name}: {}", fields[0]);
                if fields[1] == "-" {
                    assert_eq!(fields[2], "-", "{name}: invalid no-op");
                    assert_eq!(before, after, "{name}: {}", fields[0]);
                } else {
                    let start = fields[1].parse::<usize>().unwrap();
                    let end = fields[2].parse::<usize>().unwrap();
                    assert_eq!(
                        unique_deletion(&before, &after),
                        [(start, end)],
                        "{name}: {}",
                        fields[0]
                    );
                    assert!(
                        !before
                            .chars()
                            .skip(start)
                            .take(end - start)
                            .any(|ch| matches!(ch, '\r' | '\n')),
                        "{name}: deletion must stay on one line"
                    );
                }
            }
            cases += 1;
        }
        assert_eq!(cases, 3);
    }

    #[test]
    fn editor_change_repeat_snapshots_match_unique_minimum_replacements() {
        let mut lines = REPLACEMENT_ORACLE.lines();
        assert_eq!(lines.next(), Some("BLOOM_EDITOR_REPLACE_REPEAT_ORACLE_V1"));
        let mut cases = 0;
        let mut edits = 0;
        let mut no_ops = 0;
        for line in lines.filter(|line| !line.is_empty()) {
            let fields = line.split('|').collect::<Vec<_>>();
            assert_eq!(fields.len(), 5, "invalid oracle row: {line}");
            let (name, raw, expected, setup, steps) = (
                fields[0],
                decode(fields[1]),
                decode(fields[2]),
                fields[3],
                fields[4],
            );
            let mut sim = SimInput::with_content(&raw);
            assert_eq!(
                sim.editor
                    .active_buffer_text()
                    .expect("active scratch page"),
                expected,
                "{name}: initial visible text"
            );
            if setup != "-" {
                sim.keys(setup);
            }
            for step in steps.split(';') {
                let fields = step.splitn(5, ',').collect::<Vec<_>>();
                assert_eq!(fields.len(), 5, "{name}: invalid step");
                let expected_after = decode(fields[4]);
                let before = sim
                    .editor
                    .active_buffer_text()
                    .expect("active scratch page");
                sim.keys(fields[0]);
                let after = sim
                    .editor
                    .active_buffer_text()
                    .expect("active scratch page");
                assert_eq!(after, expected_after, "{name}: {}", fields[0]);
                if fields[1] == "-" {
                    assert_eq!((fields[2], fields[3]), ("-", "-"), "{name}");
                    assert_eq!(before, after, "{name}: {}", fields[0]);
                    no_ops += 1;
                } else {
                    let start = fields[1].parse::<usize>().unwrap();
                    let end = fields[2].parse::<usize>().unwrap();
                    let replacement = if fields[3] == "-" {
                        String::new()
                    } else {
                        decode(fields[3])
                    };
                    assert_eq!(
                        minimum_replacements(&before, &after),
                        [(start, end, replacement.clone())],
                        "{name}: {}",
                        fields[0]
                    );
                    assert!(
                        !before
                            .chars()
                            .skip(start)
                            .take(end - start)
                            .chain(replacement.chars())
                            .any(|ch| matches!(ch, '\r' | '\n')),
                        "{name}: replacement must stay on one line"
                    );
                    edits += 1;
                }
            }
            cases += 1;
        }
        assert_eq!((cases, edits, no_ops), (2, 6, 4));
    }
}
