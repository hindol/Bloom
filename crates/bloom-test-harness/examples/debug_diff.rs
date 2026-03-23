use similar::{ChangeTag, TextDiff};

fn main() {
    // Test what word_diff produces
    let old = "## Overview";
    let new = "## Overview ^bl3y4";

    let diff = TextDiff::from_words(old, new);
    println!("Word diff: {:?} -> {:?}", old, new);
    for change in diff.iter_all_changes() {
        let tag = match change.tag() {
            ChangeTag::Equal => "CTX",
            ChangeTag::Insert => "ADD",
            ChangeTag::Delete => "REM",
        };
        print!("[{tag}:{:?}]", change.value());
    }
    println!("\n");

    let old2 = "- [ ] Read the ropey crate API docs";
    let new2 = "- [x] Read the ropey crate API docs ^k7m2x";
    let diff2 = TextDiff::from_words(old2, new2);
    println!("Word diff: {:?} -> {:?}", old2, new2);
    for change in diff2.iter_all_changes() {
        let tag = match change.tag() {
            ChangeTag::Equal => "CTX",
            ChangeTag::Insert => "ADD",
            ChangeTag::Delete => "REM",
        };
        print!("[{tag}:{:?}]", change.value());
    }
    println!();
}
