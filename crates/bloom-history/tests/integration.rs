//! Integration tests for bloom-history and vault migration.

use bloom_history::HistoryRepo;
use std::fs;
use tempfile::TempDir;

/// Helper: create a .index/ directory and open a HistoryRepo.
fn setup() -> (TempDir, HistoryRepo) {
    let dir = TempDir::new().unwrap();
    let index_dir = dir.path().join(".index");
    fs::create_dir_all(&index_dir).unwrap();
    let repo = HistoryRepo::open(&index_dir).unwrap();
    (dir, repo)
}

#[test]
fn init_creates_git_directory() {
    let (dir, _repo) = setup();
    assert!(dir.path().join(".index/.git").exists());
    assert!(dir.path().join(".index/.git/objects").exists());
    assert!(dir.path().join(".index/.git/refs").exists());
}

#[test]
fn revwalk_returns_newest_first() {
    let (_dir, repo) = setup();

    let oid1 = repo
        .commit_all(&[("page0001", "v1")], "first", Some(1000))
        .unwrap()
        .unwrap();
    let oid2 = repo
        .commit_all(&[("page0001", "v2")], "second", Some(2000))
        .unwrap()
        .unwrap();
    let oid3 = repo
        .commit_all(&[("page0001", "v3")], "third", Some(3000))
        .unwrap()
        .unwrap();

    let history = repo.page_history(Some("page0001"), 100).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].oid, oid3);
    assert_eq!(history[1].oid, oid2);
    assert_eq!(history[2].oid, oid1);
}

#[test]
fn rename_survival_via_stable_uuid() {
    let (_dir, repo) = setup();

    // Commit "page A" under uuid "aabb0001".
    repo.commit_all(
        &[("aabb0001", "---\ntitle: Page A\n---\nContent v1")],
        "create page A",
        Some(1000),
    )
    .unwrap();

    // Simulate rename: same UUID, different title in frontmatter.
    repo.commit_all(
        &[("aabb0001", "---\ntitle: Renamed Page\n---\nContent v1")],
        "renamed Page A → Renamed Page",
        Some(2000),
    )
    .unwrap();

    // Edit after rename.
    repo.commit_all(
        &[(
            "aabb0001",
            "---\ntitle: Renamed Page\n---\nContent v2 after rename",
        )],
        "edited after rename",
        Some(3000),
    )
    .unwrap();

    // Full history for this UUID — should show all 3 versions.
    let history = repo.page_history(Some("aabb0001"), 100).unwrap();
    assert_eq!(history.len(), 3);

    // Can retrieve original content by OID.
    let first_oid = &history[2].oid;
    let content = repo.blob_at(first_oid, "aabb0001").unwrap().unwrap();
    assert!(content.contains("Content v1"));
    assert!(content.contains("title: Page A"));
}

#[test]
fn backdated_commits_preserve_order() {
    let (_dir, repo) = setup();

    // Use the commit_at pattern (explicit timestamps) to simulate history.
    let day1 = 1_709_251_200; // 2024-03-01 00:00:00 UTC
    let day2 = day1 + 86_400; // 2024-03-02
    let day3 = day2 + 86_400; // 2024-03-03

    repo.commit_all(
        &[("journal01", "Day 1 journal entry")],
        "2024-03-01 — journal",
        Some(day1),
    )
    .unwrap();

    repo.commit_all(
        &[
            ("journal01", "Day 1 journal entry"),
            ("journal02", "Day 2 journal entry"),
        ],
        "2024-03-02 — journal",
        Some(day2),
    )
    .unwrap();

    repo.commit_all(
        &[
            ("journal01", "Day 1 journal entry"),
            ("journal02", "Day 2 journal entry"),
            ("journal03", "Day 3 journal entry"),
        ],
        "2024-03-03 — journal",
        Some(day3),
    )
    .unwrap();

    // Revwalk returns newest first.
    let all = repo.page_history(None, 100).unwrap();
    // 4 total: initial empty + 3 real
    assert_eq!(all.len(), 4);
    assert!(all[0].timestamp > all[1].timestamp);
    assert!(all[1].timestamp > all[2].timestamp);
}

#[test]
fn multiple_pages_tracked_independently() {
    let (_dir, repo) = setup();

    repo.commit_all(
        &[("page_a", "A content v1"), ("page_b", "B content v1")],
        "initial",
        Some(1000),
    )
    .unwrap();

    // Only modify page_a.
    repo.commit_all(&[("page_a", "A content v2")], "update A", Some(2000))
        .unwrap();

    // Only modify page_b.
    repo.commit_all(&[("page_b", "B content v2")], "update B", Some(3000))
        .unwrap();

    // page_a history: initial + update A = 2 commits.
    let hist_a = repo.page_history(Some("page_a"), 100).unwrap();
    assert_eq!(hist_a.len(), 2);

    // page_b history: initial + update B = 2 commits.
    let hist_b = repo.page_history(Some("page_b"), 100).unwrap();
    assert_eq!(hist_b.len(), 2);
}

#[test]
fn no_change_commit_is_skipped() {
    let (_dir, repo) = setup();

    repo.commit_all(&[("p1", "content")], "first", Some(1000))
        .unwrap()
        .unwrap();

    // Same content → should return None (no commit created).
    let result = repo
        .commit_all(&[("p1", "content")], "duplicate", Some(2000))
        .unwrap();
    assert!(result.is_none());

    // Only 2 commits total (initial empty + 1 real).
    let history = repo.page_history(None, 100).unwrap();
    assert_eq!(history.len(), 2);
}
