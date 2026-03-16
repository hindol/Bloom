//! Animation recording tests — produce JSON frame data for animated docs.
//!
//! Each test records a feature scenario and saves frames to
//! `target/animations/{name}.json`. A renderer script converts these
//! to SVG → GIF for the documentation site.
//!
//! Run: `cargo test -p bloom-test-harness --test animations`

use bloom_test_harness::{FrameRecorder, SimInput};

#[test]
fn anim_basic_editing() {
    let content = "# My Notes\n\nHello world.\n";
    let mut rec = FrameRecorder::new(SimInput::with_content(content));

    // Show initial state
    rec.pause(800);

    // Navigate to "world"
    rec.step("2j");       // down to "Hello world."
    rec.step("w");        // on "world"
    rec.caption("Change a word with ciw");
    rec.step("ciw");      // change inner word
    rec.step_type("Bloom");
    rec.step("<Esc>");
    rec.pause(600);

    rec.caption("Undo with u");
    rec.step("u");
    rec.pause(800);

    rec.clear_caption();
    rec.pause(500);

    let path = rec.save("basic-editing");
    assert!(path.exists());
    assert!(rec.frame_count() > 5);
    eprintln!("Total duration: {}ms, {} frames", rec.total_duration_ms(), rec.frame_count());
}

#[test]
fn anim_search() {
    let content = "# Tasks\n\n- [ ] Buy milk\n- [ ] Review code\n- [x] Ship feature\n- [ ] Buy eggs\n";
    let mut rec = FrameRecorder::new(SimInput::with_content(content));

    rec.pause(600);
    rec.caption("Search with /");
    rec.step("/");
    rec.step_type("Buy");
    rec.pause(800);

    rec.caption("Jump to next match with n");
    rec.step("<CR>");
    rec.pause(300);
    rec.step("n");
    rec.pause(600);

    rec.caption("Press Escape to dismiss");
    rec.step("<Esc>");
    rec.pause(500);

    rec.clear_caption();
    let path = rec.save("search");
    assert!(path.exists());
}

#[test]
fn anim_block_history() {
    let content = "---\nid: demo01\ntitle: \"Demo\"\n---\n\n- [ ] Original task ^task1\n\nSome notes.\n";
    let mut rec = FrameRecorder::new(SimInput::with_content(content));

    // Navigate to the task line
    rec.step("6gg");
    rec.pause(400);

    // Edit the task
    rec.caption("Edit a task");
    rec.step("03w");    // on "Original"
    rec.step("ciw");
    rec.step_type("Updated");
    rec.step("<Esc>");
    rec.pause(600);

    // Open block history
    rec.caption("SPC H b — block history");
    rec.step("SPC H b");
    rec.pause(800);

    // Scrub to older version
    rec.caption("Scrub left to see older versions");
    rec.step("h");
    rec.pause(600);

    // Close
    rec.step("q");
    rec.pause(300);

    rec.clear_caption();
    let path = rec.save("block-history");
    assert!(path.exists());
}
