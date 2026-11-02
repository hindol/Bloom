#[cfg(test)]
mod tests {
    use bloom_test_harness::SimInput;

    const ORACLE: &str = include_str!("../../../testdata/adr3_editor_repeat.v1");

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

    #[test]
    fn deletion_inference_detects_ambiguous_and_non_deletion_changes() {
        assert_eq!(unique_deletion("aaa", "aa"), [(0, 1), (1, 2), (2, 3)]);
        assert!(unique_deletion("abc", "axc").is_empty());
        assert!(unique_deletion("abc", "abcd").is_empty());
        assert_eq!(unique_deletion("a\nb", "ab"), [(1, 2)]);
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
}
