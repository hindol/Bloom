/// Check if content contains git merge conflict markers.
pub(crate) fn has_merge_conflicts(content: &str) -> bool {
    let has_start = content.lines().any(|l| l.starts_with("<<<<<<<"));
    let has_mid = content.lines().any(|l| l.starts_with("======="));
    let has_end = content.lines().any(|l| l.starts_with(">>>>>>>"));
    has_start && has_mid && has_end
}

/// Generate .gitignore content for a Bloom vault.
pub(crate) fn gitignore_content() -> &'static str {
    "\
.index.db
*.tmp
"
}
