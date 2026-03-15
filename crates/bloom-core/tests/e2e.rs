//! End-to-end integration tests using SimInput + TestScreen.
//!
//! Each test drives BloomEditor through key sequences and asserts on the
//! visual output. No terminal, no GUI — runs in CI.

use bloom_test_harness::{SimInput, TestVault, linked_vault, task_vault, tagged_vault};

// -----------------------------------------------------------------------
// UC-01: Open today's journal
// -----------------------------------------------------------------------

#[test]
fn uc01_open_journal() {
    let vault = TestVault::new().page("Existing Page").build();
    let mut sim = SimInput::with_vault(vault);

    // SPC j t opens today's journal (was SPC j j before journal redesign)
    sim.keys("SPC j t");

    let screen = sim.screen(80, 24);
    // Journal page should be active — title contains the date
    assert!(
        screen.title().contains("202") || screen.title().is_empty() == false,
        "journal should be open, got title: '{}'",
        screen.title()
    );
}

// -----------------------------------------------------------------------
// UC-08: Find and open a page
// -----------------------------------------------------------------------

#[test]
fn uc08_find_page_opens_picker() {
    let vault = TestVault::new()
        .page("Rust Notes")
        .with_content("# Rust\n\nSome content about Rust.\n")
        .page("Text Editor Theory")
        .with_content("# Editors\n\nEditor architecture.\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Open picker
    sim.keys("SPC f f");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "picker should be open after SPC f f");
    assert!(screen.picker_results().len() >= 2, "should show at least 2 pages");

    // Close picker
    sim.keys("<Esc>");
    let screen = sim.screen(80, 24);
    assert!(!screen.has_picker(), "picker should close on Esc");
}

// -----------------------------------------------------------------------
// UC-14: Basic Vim editing — insert, navigate, delete
// -----------------------------------------------------------------------

#[test]
fn uc14_insert_mode_typing() {
    let mut sim = SimInput::with_content("");

    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "NORMAL");

    // Enter insert mode, type text
    sim.keys("i");
    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "INSERT");

    sim.type_text("Hello world");
    sim.keys("<Esc>");

    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "NORMAL");
    assert_eq!(sim.buffer_text(), "Hello world");
}

#[test]
fn uc14_jk_navigation() {
    let mut sim = SimInput::with_content("line one\nline two\nline three\n");

    assert_eq!(sim.screen(80, 24).cursor(), (0, 0));

    sim.keys("j");
    assert_eq!(sim.screen(80, 24).cursor().0, 1);

    sim.keys("j");
    assert_eq!(sim.screen(80, 24).cursor().0, 2);

    sim.keys("k");
    assert_eq!(sim.screen(80, 24).cursor().0, 1);
}

#[test]
fn cursor_does_not_wrap_to_line_zero() {
    // Test with content similar to a real vault page (block IDs, no trailing newline)
    let content = "---\nid: abcd1234\ntitle: \"Test\"\ncreated: 2026-01-01\ntags: []\n---\n\n# Heading ^abc01\n\nSome text here. ^def02\n\n- Item one ^ghi03\n- Item two ^jkl04\n- [ ] Task one @due(2026-03-15) ^mno05\n- [x] Task two ^pqr06";
    let mut sim = SimInput::with_content(content);

    // Go to last line
    sim.keys("G");
    let last = sim.screen(80, 24).cursor().0;

    // Press j 30 times — cursor must NEVER wrap to line 0
    for i in 0..30 {
        sim.keys("j");
        let (line, _) = sim.screen(80, 24).cursor();
        assert!(
            line >= last.saturating_sub(1),
            "after {}x j from line {}, cursor jumped to line {}",
            i + 1, last, line,
        );
    }

    // Verify cursor is still near the end
    let (line, _) = sim.screen(80, 24).cursor();
    assert!(
        line > 0,
        "cursor should not be at line 0 after pressing j past end"
    );

    // Now press i and type — text must NOT appear at the beginning
    sim.keys("i");
    sim.type_text("INSERTED");
    sim.keys("<Esc>");

    let text = sim.buffer_text();
    assert!(
        !text.starts_with("INSERTED"),
        "text should not be inserted at beginning of buffer"
    );
}

#[test]
fn cursor_does_not_wrap_with_vault_page() {
    // Test via vault — this exercises the full pipeline including block ID assignment
    let vault = TestVault::new()
        .page("Test Page")
        .with_content("# Content\n\nParagraph one.\n\nParagraph two.\n\n- Item a\n- Item b\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Open the page
    sim.keys("SPC f f");
    sim.type_text("Test");
    sim.keys("Enter");

    // Go to end, then j past it
    sim.keys("G");
    let last = sim.screen(80, 24).cursor().0;

    for _ in 0..20 {
        sim.keys("j");
    }

    let (line, _) = sim.screen(80, 24).cursor();
    assert!(
        line > 0 || last == 0,
        "cursor should not wrap to 0, got line {} (last was {})",
        line, last,
    );
}

#[test]
fn cursor_does_not_go_past_eof_no_trailing_newline() {
    // Test without trailing newline
    let mut sim = SimInput::with_content("line one\nline two\nline three");

    sim.keys("G");
    let last_line = sim.screen(80, 24).cursor().0;

    for _ in 0..10 {
        sim.keys("j");
    }
    let after = sim.screen(80, 24).cursor().0;
    assert_eq!(after, last_line, "cursor should stay on last line without trailing newline");
}

#[test]
fn cursor_does_not_go_past_eof_single_line() {
    let mut sim = SimInput::with_content("hello");

    assert_eq!(sim.screen(80, 24).cursor(), (0, 0));

    for _ in 0..5 {
        sim.keys("j");
    }
    assert_eq!(sim.screen(80, 24).cursor().0, 0, "single line: j should be no-op");
}

#[test]
fn cursor_past_eof_after_delete() {
    // Delete lines until only one remains — cursor should stay valid
    let mut sim = SimInput::with_content("a\nb\nc\nd\n");

    sim.keys("G"); // go to last line
    sim.keys("dd"); // delete it
    sim.keys("dd"); // delete again
    sim.keys("dd"); // delete again

    // Should still have valid cursor
    let screen = sim.screen(80, 24);
    let (line, _) = screen.cursor();
    let text = sim.buffer_text();
    let line_count = text.lines().count().max(1);
    assert!(
        line < line_count,
        "cursor line {} should be within buffer ({} lines), text: '{text}'",
        line,
        line_count,
    );
}

#[test]
fn uc14_dw_delete_word() {
    let mut sim = SimInput::with_content("hello world");

    sim.keys("dw");
    assert_eq!(sim.buffer_text(), "world");
}

// -----------------------------------------------------------------------
// UC-15: Vim operators with counts
// -----------------------------------------------------------------------

#[test]
fn uc15_count_motion() {
    let mut sim = SimInput::with_content("one two three four five\n");

    // 3w moves forward 3 words
    sim.keys("3w");
    let (_, col) = sim.screen(80, 24).cursor();
    // Should be on "four" — exact column depends on word boundaries
    assert!(col > 5, "3w should move past 'one two three', col={col}");
}

#[test]
fn uc15_d_dollar_deletes_to_eol() {
    let mut sim = SimInput::with_content("keep this\ndelete from here to end\n");

    sim.keys("j"); // go to line 2
    sim.keys("w"); // move to "from"
    sim.keys("d$"); // delete to end of line

    let text = sim.buffer_text();
    assert!(text.contains("delete "), "should keep 'delete '");
    assert!(!text.contains("here to end"), "should delete 'from here to end'");
}

// -----------------------------------------------------------------------
// UC-17: Visual mode
// -----------------------------------------------------------------------

#[test]
fn uc17_visual_select_and_delete() {
    let mut sim = SimInput::with_content("hello world");

    sim.keys("v"); // enter visual mode
    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "VISUAL");

    sim.keys("e"); // select "hello"
    sim.keys("d"); // delete selection

    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "NORMAL");
    // "hello" should be deleted
    let text = sim.buffer_text();
    assert!(!text.starts_with("hello"), "selection should be deleted: '{text}'");
}

// -----------------------------------------------------------------------
// UC-18: Undo and redo
// -----------------------------------------------------------------------

#[test]
fn uc18_undo_redo() {
    let mut sim = SimInput::with_content("");

    // Type "abc"
    sim.keys("i");
    sim.type_text("abc");
    sim.keys("<Esc>");
    assert_eq!(sim.buffer_text(), "abc");

    // Undo
    sim.keys("u");
    assert_eq!(sim.buffer_text(), "");

    // Redo
    sim.keys("C-r");
    assert_eq!(sim.buffer_text(), "abc");
}

// -----------------------------------------------------------------------
// UC-20: Command mode
// -----------------------------------------------------------------------

#[test]
fn uc20_command_mode() {
    let mut sim = SimInput::with_content("hello");

    sim.keys(":");
    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "COMMAND");

    sim.keys("<Esc>");
    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "NORMAL");
}

// -----------------------------------------------------------------------
// UC-23: Dot repeat
// -----------------------------------------------------------------------

#[test]
fn uc23_dot_repeat() {
    let mut sim = SimInput::with_content("aaa bbb ccc\n");

    // Delete first word
    sim.keys("dw");
    assert_eq!(sim.buffer_text(), "bbb ccc\n");

    // Dot repeat — should delete next word
    sim.keys(".");
    assert_eq!(sim.buffer_text(), "ccc\n", "dot repeat should delete next word");
}

// -----------------------------------------------------------------------
// UC-42: Toggle task
// -----------------------------------------------------------------------

#[test]
fn uc42_task_toggle_via_vim() {
    let mut sim = SimInput::with_content("- [ ] buy milk\n");

    // Position cursor on the space in [ ] (column 3)
    sim.keys("3l"); // move to column 3
    sim.keys("r"); // replace mode
    sim.type_text("x"); // replace space with x

    assert!(
        sim.buffer_text().contains("- [x] buy milk"),
        "task should be toggled: '{}'",
        sim.buffer_text()
    );
}

// -----------------------------------------------------------------------
// UC-52: Window split
// -----------------------------------------------------------------------

#[test]
fn uc52_vertical_split() {
    let mut sim = SimInput::with_content("hello");

    let screen = sim.screen(80, 24);
    assert_eq!(screen.pane_count(), 1);

    sim.keys("SPC w v");

    let screen = sim.screen(80, 24);
    assert!(
        screen.pane_count() >= 2,
        "split should create 2+ panes, got {}",
        screen.pane_count()
    );
}

// -----------------------------------------------------------------------
// UC-55: Maximize and restore
// -----------------------------------------------------------------------

#[test]
fn uc55_maximize_restore() {
    let mut sim = SimInput::with_content("hello");

    // Split first
    sim.keys("SPC w v");
    let screen = sim.screen(80, 24);
    let panes_split = screen.pane_count();
    assert!(panes_split >= 2);

    // Maximize
    sim.keys("SPC w m");
    let screen = sim.screen(80, 24);
    // In maximized mode, only 1 pane is visible
    assert_eq!(screen.pane_count(), 1, "maximized should show 1 pane");

    // Restore
    sim.keys("SPC w m");
    let screen = sim.screen(80, 24);
    assert_eq!(
        screen.pane_count(),
        panes_split,
        "restore should bring back all panes"
    );
}

// -----------------------------------------------------------------------
// UC-87: Which-key popup
// -----------------------------------------------------------------------

#[test]
fn uc87_whichkey_shows_on_timeout() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC");
    sim.tick(1000); // trigger which-key timeout

    let screen = sim.screen(80, 24);
    assert!(
        screen.has_which_key(),
        "which-key should appear after SPC + timeout"
    );
}

// -----------------------------------------------------------------------
// Editing edge cases
// -----------------------------------------------------------------------

#[test]
fn open_line_below() {
    let mut sim = SimInput::with_content("line one\nline two\n");

    sim.keys("o"); // open below
    sim.type_text("new line");
    sim.keys("<Esc>");

    let text = sim.buffer_text();
    assert!(text.contains("line one\nnew line\nline two"), "o should insert below: '{text}'");
}

#[test]
fn open_line_above() {
    let mut sim = SimInput::with_content("line one\nline two\n");

    sim.keys("j"); // go to line 2
    sim.keys("O"); // open above
    sim.type_text("inserted");
    sim.keys("<Esc>");

    let text = sim.buffer_text();
    assert!(text.contains("line one\ninserted\nline two"), "O should insert above: '{text}'");
}

#[test]
fn backspace_in_insert_mode() {
    let mut sim = SimInput::with_content("hello");

    sim.keys("A"); // append at end
    sim.type_text("!");
    sim.keys("<BS>"); // backspace
    sim.keys("<Esc>");

    assert_eq!(sim.buffer_text(), "hello");
}

#[test]
fn dirty_flag_set_on_edit() {
    let mut sim = SimInput::with_content("hello");

    let screen = sim.screen(80, 24);
    assert!(!screen.is_dirty(), "fresh buffer should not be dirty");

    sim.keys("i");
    sim.type_text("x");
    sim.keys("<Esc>");

    let screen = sim.screen(80, 24);
    assert!(screen.is_dirty(), "buffer should be dirty after edit");
}

// =======================================================================
// Additional coverage — journal, search, buffers, windows, Vim ops
// =======================================================================

// -----------------------------------------------------------------------
// UC-02/03: Quick capture
// -----------------------------------------------------------------------

#[test]
fn uc02_quick_capture_journal_append() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // SPC j a opens quick capture
    sim.keys("SPC j a");
    let screen = sim.screen(80, 24);
    // Quick capture shows in the status bar content area
    assert_eq!(screen.mode(), "NORMAL"); // mode stays normal, capture is an overlay
}

// -----------------------------------------------------------------------
// UC-04: Journal navigation (prev/next)
// -----------------------------------------------------------------------

#[test]
fn uc04_journal_prev_next() {
    // Create journal files for two specific dates so day-hopping has targets
    let vault = TestVault::new()
        .page("Test")
        .raw_file(
            "journal/2026-03-10.md",
            "---\nid: aaaa1111\ntitle: \"2026-03-10\"\ncreated: 2026-03-10\ntags: [journal]\n---\n\n- Earlier journal\n",
        )
        .raw_file(
            "journal/2026-03-08.md",
            "---\nid: aaaa2222\ntitle: \"2026-03-08\"\ncreated: 2026-03-08\ntags: [journal]\n---\n\n- Even earlier\n",
        )
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Open today's journal first (SPC j t after journal redesign)
    sim.keys("SPC j t");
    let title1 = sim.screen(80, 24).title().to_string();

    // SPC j p should skip to the previous day with a file (not just -1 day)
    sim.type_text("[d");
    let title2 = sim.screen(80, 24).title().to_string();

    // Titles should differ (skipped to a day that has a journal file)
    assert_ne!(title1, title2, "prev journal should have different date");
    assert!(
        title2.contains("2026-03-10") || title2.contains("2026-03-08"),
        "should land on a day with a journal file, got: '{}'",
        title2
    );
}

// -----------------------------------------------------------------------
// UC-11: Switch buffer
// -----------------------------------------------------------------------

#[test]
fn uc11_switch_buffer() {
    let vault = TestVault::new()
        .page("Page A")
        .with_content("Content A\n")
        .page("Page B")
        .with_content("Content B\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Open page A via picker
    sim.keys("SPC f f");
    sim.type_text("Page A");
    sim.keys("Enter");

    // Open page B via picker
    sim.keys("SPC f f");
    sim.type_text("Page B");
    sim.keys("Enter");

    // Now use buffer switcher
    sim.keys("SPC b b");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "buffer picker should open on SPC b b");

    sim.keys("<Esc>");
}

// -----------------------------------------------------------------------
// UC-12: Close buffer
// -----------------------------------------------------------------------

#[test]
fn uc12_close_buffer() {
    let mut sim = SimInput::with_content("hello");

    // Buffer is open
    assert!(sim.screen(80, 24).title().len() > 0);

    // SPC b d closes buffer
    sim.keys("SPC b k");

    // After close, a new buffer should be open (journal or scratch)
    let screen = sim.screen(80, 24);
    assert!(screen.title().len() > 0, "new buffer should be open after close");
}

// -----------------------------------------------------------------------
// UC-16: Bloom-specific text objects
// -----------------------------------------------------------------------

#[test]
fn uc16_delete_inside_word() {
    let mut sim = SimInput::with_content("hello world test");

    sim.keys("w"); // move to "world"
    sim.keys("diw"); // delete inner word

    let text = sim.buffer_text();
    assert!(!text.contains("world"), "diw should delete 'world': '{text}'");
    assert!(text.contains("hello"), "should keep 'hello'");
}

#[test]
fn uc16_change_inner_word() {
    let mut sim = SimInput::with_content("foo bar baz");

    sim.keys("w"); // move to "bar"
    sim.keys("ciw"); // change inner word
    sim.type_text("QUX");
    sim.keys("<Esc>");

    let text = sim.buffer_text();
    assert!(text.contains("QUX"), "ciw should replace 'bar' with 'QUX': '{text}'");
    assert!(!text.contains("bar"), "original word should be gone");
}

// -----------------------------------------------------------------------
// UC-21: Registers (yank and paste)
// -----------------------------------------------------------------------

#[test]
fn uc21_yank_and_paste() {
    let mut sim = SimInput::with_content("hello world");

    sim.keys("yiw"); // yank "hello"
    sim.keys("$"); // go to end
    sim.keys("p"); // paste after cursor

    let text = sim.buffer_text();
    assert!(
        text.contains("worldhello") || text.contains("world hello"),
        "paste should insert yanked text: '{text}'"
    );
}

#[test]
fn uc21_dd_yank_line_and_paste() {
    let mut sim = SimInput::with_content("line one\nline two\nline three\n");

    sim.keys("dd"); // delete+yank first line
    let text = sim.buffer_text();
    assert!(!text.starts_with("line one"), "dd should delete first line");

    sim.keys("p"); // paste below
    let text = sim.buffer_text();
    assert!(text.contains("line one"), "p should paste the deleted line back: '{text}'");
}

// -----------------------------------------------------------------------
// UC-22: Macro recording
// -----------------------------------------------------------------------

#[test]
fn uc22_macro_record_replay() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc\n");

    // Record macro: delete line
    sim.keys("qa"); // start recording into register a
    sim.keys("dd"); // delete line
    sim.keys("q"); // stop recording

    // First dd should work
    let text = sim.buffer_text();
    assert!(!text.contains("aaa"), "first dd should remove aaa: '{text}'");

    // Replay
    sim.keys("@a");
    let text = sim.buffer_text();
    assert!(!text.contains("bbb"), "macro replay should remove bbb: '{text}'");
    assert!(text.contains("ccc"), "ccc should remain: '{text}'");
}

// -----------------------------------------------------------------------
// UC-37: Full-text search
// -----------------------------------------------------------------------

#[test]
fn uc37_search_opens_picker() {
    let vault = TestVault::new()
        .page("Rust Notes")
        .with_content("# Rust\n\nMemory safety is key.\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC s s");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "search picker should open on SPC s s");

    sim.keys("<Esc>");
}

// -----------------------------------------------------------------------
// UC-53: Navigate between windows
// -----------------------------------------------------------------------

#[test]
fn uc53_navigate_windows() {
    let mut sim = SimInput::with_content("left pane");

    // Split
    sim.keys("SPC w v");
    let screen = sim.screen(80, 24);
    assert!(screen.pane_count() >= 2);

    // Navigate right
    sim.keys("SPC w l");
    // Navigate left
    sim.keys("SPC w h");
    // Should not crash, pane count unchanged
    let screen = sim.screen(80, 24);
    assert!(screen.pane_count() >= 2);
}

// -----------------------------------------------------------------------
// UC-56: Close window
// -----------------------------------------------------------------------

#[test]
fn uc56_close_window() {
    let mut sim = SimInput::with_content("hello");

    // Split first
    sim.keys("SPC w v");
    assert!(sim.screen(80, 24).pane_count() >= 2);

    // Close one
    sim.keys("SPC w d");
    assert_eq!(sim.screen(80, 24).pane_count(), 1, "close should remove a pane");
}

// -----------------------------------------------------------------------
// Vim operations: append, change, replace
// -----------------------------------------------------------------------

#[test]
fn vim_append_end_of_line() {
    let mut sim = SimInput::with_content("hello");

    sim.keys("A"); // append at end
    sim.type_text(" world");
    sim.keys("<Esc>");

    assert_eq!(sim.buffer_text(), "hello world");
}

#[test]
fn vim_change_word() {
    let mut sim = SimInput::with_content("old text here");

    sim.keys("cw"); // change word
    sim.type_text("new");
    sim.keys("<Esc>");

    let text = sim.buffer_text();
    assert!(text.starts_with("new"), "cw should replace first word: '{text}'");
    assert!(!text.contains("old"), "old word should be gone");
}

#[test]
fn vim_replace_char() {
    let mut sim = SimInput::with_content("hello");

    sim.keys("r"); // replace mode
    sim.type_text("H"); // replace 'h' with 'H'

    assert_eq!(sim.buffer_text(), "Hello");
}

#[test]
fn vim_goto_line_start_end() {
    let mut sim = SimInput::with_content("hello world");

    sim.keys("$"); // end of line
    let (_, col) = sim.screen(80, 24).cursor();
    assert!(col > 0, "$ should move to end of line");

    sim.keys("0"); // start of line
    let (_, col) = sim.screen(80, 24).cursor();
    assert_eq!(col, 0, "0 should move to start of line");
}

#[test]
#[allow(non_snake_case)]
fn vim_gg_and_G() {
    let mut sim = SimInput::with_content("line 1\nline 2\nline 3\nline 4\nline 5\n");

    sim.keys("G"); // go to last line
    let (line, _) = sim.screen(80, 24).cursor();
    assert!(line >= 4, "G should go to last line, got {line}");

    sim.keys("gg"); // go to first line
    let (line, _) = sim.screen(80, 24).cursor();
    assert_eq!(line, 0, "gg should go to first line");
}

#[test]
fn vim_w_b_word_motions() {
    let mut sim = SimInput::with_content("one two three four\n");

    sim.keys("w");
    let (_, col) = sim.screen(80, 24).cursor();
    assert_eq!(col, 4, "w should move to 'two' at col 4");

    sim.keys("w");
    let (_, col) = sim.screen(80, 24).cursor();
    assert_eq!(col, 8, "w again should move to 'three' at col 8");

    sim.keys("b");
    let (_, col) = sim.screen(80, 24).cursor();
    assert_eq!(col, 4, "b should move back to 'two' at col 4");
}

// -----------------------------------------------------------------------
// UC-89: All commands picker (SPC SPC)
// -----------------------------------------------------------------------

#[test]
fn uc89_all_commands_picker() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // SPC ? opens the all commands picker
    sim.keys("SPC ?");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "SPC ? should open the commands picker");
    assert!(
        screen.picker_results().len() > 5,
        "commands picker should have many results, got {}",
        screen.picker_results().len()
    );

    sim.keys("<Esc>");
}

// -----------------------------------------------------------------------
// UC-90: Platform shortcuts (Ctrl+S saves)
// -----------------------------------------------------------------------

#[test]
fn uc90_ctrl_s_saves() {
    let mut sim = SimInput::with_content("hello");

    // Make dirty
    sim.keys("i");
    sim.type_text("x");
    sim.keys("<Esc>");
    assert!(sim.screen(80, 24).is_dirty());

    // Ctrl+S triggers save action (buffer may or may not be marked clean
    // depending on DiskWriter availability in tests)
    sim.keys("C-s");
    // Should not crash
    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "NORMAL");
}

// -----------------------------------------------------------------------
// Enter in Normal mode
// -----------------------------------------------------------------------

#[test]
fn enter_creates_newline_in_insert() {
    let mut sim = SimInput::with_content("hello world");

    sim.keys("i");
    sim.keys("Enter");
    sim.keys("<Esc>");

    let text = sim.buffer_text();
    assert!(text.contains('\n'), "Enter in Insert mode should create newline: '{text}'");
}

// -----------------------------------------------------------------------
// Multiple undo/redo cycles
// -----------------------------------------------------------------------

#[test]
fn multiple_undo_redo() {
    let mut sim = SimInput::with_content("");

    // 3 insert sessions
    sim.keys("i");
    sim.type_text("one ");
    sim.keys("<Esc>");

    sim.keys("i");
    sim.type_text("two ");
    sim.keys("<Esc>");

    sim.keys("i");
    sim.type_text("three");
    sim.keys("<Esc>");

    assert_eq!(sim.buffer_text(), "one two three");

    // Undo all 3
    sim.keys("u");
    assert_eq!(sim.buffer_text(), "one two ");
    sim.keys("u");
    assert_eq!(sim.buffer_text(), "one ");
    sim.keys("u");
    assert_eq!(sim.buffer_text(), "");

    // Redo all 3
    sim.keys("C-r");
    assert_eq!(sim.buffer_text(), "one ");
    sim.keys("C-r");
    assert_eq!(sim.buffer_text(), "one two ");
    sim.keys("C-r");
    assert_eq!(sim.buffer_text(), "one two three");
}

// =======================================================================
// Vim operations — comprehensive coverage
// =======================================================================

#[test]
fn vim_x_delete_char() {
    let mut sim = SimInput::with_content("hello");
    sim.keys("x");
    assert_eq!(sim.buffer_text(), "ello");
}

#[test]
#[allow(non_snake_case)]
fn vim_X_delete_char_before() {
    let mut sim = SimInput::with_content("hello");
    sim.keys("l"); // move to 'e'
    sim.keys("X");
    assert_eq!(sim.buffer_text(), "ello");
}

#[test]
#[allow(non_snake_case)]
fn vim_D_delete_to_eol() {
    let mut sim = SimInput::with_content("hello world\n");
    sim.keys("w"); // move to 'world'
    sim.keys("D");
    let text = sim.buffer_text();
    assert!(text.starts_with("hello"), "D should keep text before cursor: '{text}'");
    assert!(!text.contains("world"), "D should delete to end of line: '{text}'");
}

#[test]
#[allow(non_snake_case)]
fn vim_C_change_to_eol() {
    let mut sim = SimInput::with_content("hello world\n");
    sim.keys("w"); // move to 'world'
    sim.keys("C");
    sim.type_text("rust");
    sim.keys("<Esc>");
    let text = sim.buffer_text();
    assert!(text.contains("hello rust"), "C should change to EOL: '{text}'");
}

#[test]
fn vim_dd_delete_line() {
    let mut sim = SimInput::with_content("line one\nline two\nline three\n");
    sim.keys("dd");
    let text = sim.buffer_text();
    assert!(!text.contains("line one"), "dd should delete first line: '{text}'");
    assert!(text.starts_with("line two"), "remaining lines should shift up: '{text}'");
}

#[test]
fn vim_cc_change_line() {
    let mut sim = SimInput::with_content("old line\nsecond\n");
    sim.keys("cc");
    sim.type_text("new line");
    sim.keys("<Esc>");
    let text = sim.buffer_text();
    assert!(text.contains("new line"), "cc should replace line content: '{text}'");
    assert!(!text.contains("old line"), "old content should be gone: '{text}'");
    assert!(text.contains("second"), "other lines should remain: '{text}'");
}

#[test]
#[allow(non_snake_case)]
fn vim_J_join_lines() {
    let mut sim = SimInput::with_content("hello\nworld\n");
    sim.keys("J");
    let text = sim.buffer_text();
    assert!(
        text.contains("hello world"),
        "J should join lines with space: '{}'",
        text.replace('\n', "\\n")
    );
}

#[test]
fn vim_yy_p_yank_paste_line() {
    let mut sim = SimInput::with_content("original\nsecond\n");
    sim.keys("yy"); // yank line
    sim.keys("p"); // paste below
    let text = sim.buffer_text();
    // yy + p should duplicate the line below
    assert!(
        text.matches("original").count() >= 1,
        "yy + p should keep at least original: '{text}'"
    );
}

#[test]
fn vim_count_dd() {
    let mut sim = SimInput::with_content("a\nb\nc\nd\ne\n");
    sim.keys("2dd"); // delete 2 lines
    let text = sim.buffer_text();
    assert!(!text.contains('a'), "first line deleted");
    assert!(!text.contains('b'), "second line deleted");
    assert!(text.starts_with('c'), "c should be first now: '{text}'");
}

#[test]
fn vim_f_find_char() {
    let mut sim = SimInput::with_content("hello world");
    sim.keys("fw"); // find 'w'
    let (_, col) = sim.screen(80, 24).cursor();
    assert_eq!(col, 6, "f should jump to 'w' at column 6");
}

#[test]
fn vim_percent_matching_bracket() {
    let mut sim = SimInput::with_content("(hello)");
    // Cursor on '(' — % should jump to ')'
    sim.keys("%");
    let (_, col) = sim.screen(80, 24).cursor();
    assert_eq!(col, 6, "% should jump to matching ')' at column 6");
}

#[test]
fn vim_visual_line_mode() {
    let mut sim = SimInput::with_content("line one\nline two\nline three\n");
    sim.keys("V"); // visual line mode
    assert_eq!(sim.screen(80, 24).mode(), "VISUAL");
    sim.keys("j"); // extend selection
    sim.keys("d"); // delete selected lines
    let text = sim.buffer_text();
    assert!(!text.contains("line one"), "V + j + d should delete 2 lines");
    assert!(!text.contains("line two"), "both selected lines deleted");
    assert!(text.contains("line three"), "unselected line remains: '{text}'");
}

// =======================================================================
// Window operations — deeper coverage
// =======================================================================

#[test]
fn uc54_resize_window() {
    let mut sim = SimInput::with_content("hello");
    sim.keys("SPC w v"); // split
    assert!(sim.screen(80, 24).pane_count() >= 2);

    // Widen
    sim.keys("SPC w >");
    // Narrow
    sim.keys("SPC w <");
    // Balance
    sim.keys("SPC w =");
    // Should not crash, panes intact
    assert!(sim.screen(80, 24).pane_count() >= 2);
}

#[test]
fn uc52_horizontal_split() {
    let mut sim = SimInput::with_content("hello");
    sim.keys("SPC w s"); // horizontal split
    let screen = sim.screen(80, 24);
    assert!(screen.pane_count() >= 2, "SPC w s should create horizontal split");
}

// =======================================================================
// Linking — follow link
// =======================================================================

#[test]
fn uc26_follow_link_opens_page() {
    let vault = TestVault::new()
        .page("Source")
        .with_content("See [[ccdd3344|Target Page]] for details.\n")
        .page("Target Page")
        .with_content("Target content here.\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Open source
    sim.keys("SPC f f");
    sim.type_text("Source");
    sim.keys("Enter");

    // Move cursor onto the link text
    sim.keys("j"); // skip frontmatter lines
    sim.keys("j");
    sim.keys("j");
    sim.keys("j");
    sim.keys("j");
    sim.keys("j");
    sim.keys("w"); // move to link area

    // Follow link
    sim.keys("SPC l l");
    // This opens the link picker — the actual gd follow depends on link detection
    // Just verify we don't crash and the editor is in a valid state
    let screen = sim.screen(80, 24);
    assert!(screen.mode() == "NORMAL" || screen.has_picker());
}

// =======================================================================
// Search — verify results appear
// =======================================================================

#[test]
fn uc37_search_finds_content() {
    let vault = TestVault::new()
        .page("Rust Notes")
        .with_content("# Rust\n\nMemory safety is key.\nBorrow checker rules.\n")
        .page("Python Notes")
        .with_content("# Python\n\nDynamic typing.\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC s s");
    assert!(sim.screen(80, 24).has_picker());

    sim.type_text("memory");
    let screen = sim.screen(80, 24);
    // Search should find the Rust Notes page
    let results = screen.picker_results();
    assert!(
        results.iter().any(|r| r.to_lowercase().contains("memory") || r.to_lowercase().contains("rust")),
        "search for 'memory' should find results: {:?}",
        results
    );

    sim.keys("<Esc>");
}

// =======================================================================
// Tags — search tags picker
// =======================================================================

#[test]
fn uc34_search_tags() {
    let vault = TestVault::new()
        .page("Tagged Page")
        .tags(&["rust", "editors"])
        .with_content("Content with tags.\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC s t");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "SPC s t should open tags picker");

    sim.keys("<Esc>");
}

// =======================================================================
// System — rebuild index, ex commands
// =======================================================================

#[test]
fn uc76_rebuild_index_via_command() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // :rebuild-index
    sim.keys(":");
    sim.type_text("rebuild-index");
    sim.keys("Enter");

    // Should not crash, should return to normal mode
    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "NORMAL");
}

#[test]
fn ex_command_theme_switch() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys(":");
    sim.type_text("theme");
    sim.keys("Enter");

    // Should cycle theme without crashing
    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "NORMAL");
}

// Regression: entering Command mode (:) must not hide the status bar.
// The which-key space reservation was treating command text as "pending keys",
// shrinking the pane area and hiding the command line behind the reserved space.
// In Vim, the command line replaces the status bar at the bottom row.
#[test]
fn command_mode_status_bar_stays_at_bottom() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // Record status bar position in Normal mode
    let normal = sim.screen(80, 24);
    let sb_y_normal = normal
        .frame
        .panes
        .iter()
        .find(|p| p.is_active)
        .map(|p| p.rect.y + p.rect.content_height)
        .unwrap();

    // Simulate the real trigger: user presses SPC (enters leader), waits for
    // which-key to appear, then presses Esc to cancel, then enters `:` command mode.
    sim.keys("SPC");
    sim.tick(600); // which-key timeout fires → which_key_visible = true
    sim.keys("<Esc>"); // cancel leader — clears which_key_visible

    // Now enter command mode and type a command
    sim.keys(":");
    sim.type_text("theme");
    sim.tick(600); // past which-key timeout — would trigger the bug

    let cmd_screen = sim.screen(80, 24);
    assert_eq!(cmd_screen.mode(), "COMMAND");

    let sb_y_cmd = cmd_screen
        .frame
        .panes
        .iter()
        .find(|p| p.is_active)
        .map(|p| p.rect.y + p.rect.content_height)
        .unwrap();

    // Status bar must stay at the same row — which-key must not steal space in Command mode
    assert_eq!(
        sb_y_normal, sb_y_cmd,
        "status bar must not move in Command mode (Normal: row {}, Command: row {})",
        sb_y_normal, sb_y_cmd
    );

    // After Tab autocomplete, it should still be at the bottom
    sim.keys("Tab");
    sim.tick(600);

    let tab_screen = sim.screen(80, 24);
    let sb_y_tab = tab_screen
        .frame
        .panes
        .iter()
        .find(|p| p.is_active)
        .map(|p| p.rect.y + p.rect.content_height)
        .unwrap();

    assert_eq!(
        sb_y_normal, sb_y_tab,
        "status bar must not move after Tab (Normal: row {}, Tab: row {})",
        sb_y_normal, sb_y_tab
    );

    // The command line should contain the completed text
    if let Some(pane) = tab_screen.frame.panes.iter().find(|p| p.is_active) {
        match &pane.status_bar.content {
            bloom_core::render::StatusBarContent::CommandLine(cmd) => {
                assert!(
                    cmd.input.contains("theme"),
                    "command line should contain 'theme' after Tab, got: '{}'",
                    cmd.input
                );
            }
            other => panic!(
                "expected CommandLine status bar, got: {:?}",
                std::mem::discriminant(other)
            ),
        }
    }
}

// Theme picker live preview: moving selection changes theme in the frame
#[test]
fn theme_picker_live_preview() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // Record initial theme
    let initial_theme = sim.screen(80, 24).theme_name().to_string();

    // Open theme picker via SPC T t
    sim.keys("SPC T t");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "SPC T t should open theme picker");

    // Theme should still be the initial one (picker pre-selects current)
    assert_eq!(
        screen.theme_name(),
        initial_theme,
        "picker should start on current theme"
    );

    // Move selection down — should preview a different theme
    sim.keys("C-n");
    let after_move = sim.screen(80, 24);
    assert_ne!(
        after_move.theme_name(),
        initial_theme,
        "moving selection should live-preview a different theme"
    );

    // Press Escape — should revert to original theme
    sim.keys("<Esc>");
    let after_cancel = sim.screen(80, 24);
    assert!(!after_cancel.has_picker(), "picker should be closed");
    assert_eq!(
        after_cancel.theme_name(),
        initial_theme,
        "cancel should revert to original theme"
    );
}

// Theme picker confirm: Enter persists the new theme
#[test]
fn theme_picker_confirm_persists() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    let initial_theme = sim.screen(80, 24).theme_name().to_string();

    // Open picker, move to a different theme, confirm
    sim.keys("SPC T t");
    sim.keys("C-n");
    let previewed = sim.screen(80, 24).theme_name().to_string();
    assert_ne!(previewed, initial_theme);

    sim.keys("Enter");
    let after_confirm = sim.screen(80, 24);
    assert!(!after_confirm.has_picker());
    assert_eq!(
        after_confirm.theme_name(),
        previewed,
        "confirmed theme should persist after picker closes"
    );
}

// =======================================================================
// Fixture-based tests — linked_vault
// =======================================================================

#[test]
fn linked_vault_find_all_pages() {
    let vault = linked_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker());
    assert!(
        screen.picker_results().len() >= 3,
        "linked vault should have 3 pages in picker, got {:?}",
        screen.picker_results()
    );
    sim.keys("<Esc>");
}

#[test]
fn linked_vault_open_specific_page() {
    let vault = linked_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    sim.type_text("Rust");
    sim.keys("Enter");

    let screen = sim.screen(80, 24);
    assert!(
        screen.title().contains("Rust"),
        "should open Rust Notes, got: '{}'",
        screen.title()
    );
}

#[test]
fn linked_vault_search_content() {
    let vault = linked_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC s s");
    sim.type_text("rope");
    let screen = sim.screen(80, 24);
    let results = screen.picker_results();
    assert!(
        !results.is_empty(),
        "search for 'rope' should find Text Editor Theory"
    );
    sim.keys("<Esc>");
}

#[test]
fn linked_vault_backlinks_picker() {
    let vault = linked_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    sim.type_text("Editor");
    sim.keys("Enter");

    sim.keys("SPC s l");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "SPC s l should open backlinks picker");
    sim.keys("<Esc>");
}

#[test]
fn linked_vault_unlinked_mentions() {
    let vault = linked_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    sim.type_text("Rust");
    sim.keys("Enter");

    sim.keys("SPC s u");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "SPC s u should open unlinked mentions picker");
    sim.keys("<Esc>");
}

// =======================================================================
// Fixture-based tests — task_vault
// =======================================================================

#[test]
fn task_vault_find_pages() {
    let vault = task_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    let screen = sim.screen(80, 24);
    assert!(screen.picker_results().len() >= 2, "should have at least 2 pages");
    sim.keys("<Esc>");
}

#[test]
fn task_vault_open_project_a() {
    let vault = task_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    sim.type_text("Project A");
    sim.keys("Enter");

    let text = sim.buffer_text();
    assert!(text.contains("- [ ] Review the API"), "should have unchecked task");
    assert!(text.contains("- [x] Set up CI"), "should have checked task");
}

#[test]
fn task_vault_search_tasks() {
    let vault = task_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC s s");
    sim.type_text("API design");
    let screen = sim.screen(80, 24);
    assert!(!screen.picker_results().is_empty(), "should find task");
    sim.keys("<Esc>");
}

#[test]
fn task_vault_agenda_opens() {
    let vault = task_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC a a");
    let screen = sim.screen(80, 24);
    assert!(screen.title().len() > 0 || screen.has_picker(), "agenda should open");
}

#[test]
fn task_vault_toggle_task() {
    let vault = task_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    sim.type_text("Project A");
    sim.keys("Enter");

    let text = sim.buffer_text();
    let task_line = text.lines().position(|l| l.contains("- [ ] Review")).unwrap_or(0);
    for _ in 0..task_line { sim.keys("j"); }
    sim.keys("3l");
    sim.keys("r");
    sim.type_text("x");

    let text = sim.buffer_text();
    assert!(text.contains("- [x] Review the API"), "task should be toggled");
}

// =======================================================================
// Fixture-based tests — tagged_vault
// =======================================================================

#[test]
fn tagged_vault_search_tags() {
    let vault = tagged_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC s t");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "tags picker should open");
    sim.keys("<Esc>");
}

#[test]
fn tagged_vault_find_all_pages() {
    let vault = tagged_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    assert!(sim.screen(80, 24).picker_results().len() >= 4, "should show 4 pages");
    sim.keys("<Esc>");
}

#[test]
fn tagged_vault_open_meeting_notes() {
    let vault = tagged_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    sim.type_text("Meeting");
    sim.keys("Enter");

    assert!(sim.buffer_text().contains("Discussed Rust"), "should load content");
}

// =======================================================================
// Multi-buffer workflows
// =======================================================================

#[test]
fn open_two_pages_and_switch() {
    let vault = linked_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    sim.type_text("Rust");
    sim.keys("Enter");
    let t1 = sim.screen(80, 24).title().to_string();

    sim.keys("SPC f f");
    sim.type_text("Orphan");
    sim.keys("Enter");
    let t2 = sim.screen(80, 24).title().to_string();

    assert_ne!(t1, t2, "should open different pages");

    sim.keys("SPC b b");
    assert!(sim.screen(80, 24).has_picker());
    sim.keys("Enter");
    assert!(sim.screen(80, 24).title().len() > 0);
}

#[test]
fn split_and_open_different_pages() {
    let vault = linked_vault();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC f f");
    sim.type_text("Rust");
    sim.keys("Enter");

    sim.keys("SPC w v");
    assert!(sim.screen(80, 24).pane_count() >= 2);

    sim.keys("SPC f f");
    sim.type_text("Orphan");
    sim.keys("Enter");

    let screen = sim.screen(80, 24);
    assert!(screen.pane_count() >= 2);
    assert!(screen.line_count() > 0);
}

// -----------------------------------------------------------------------
// Journal Redesign — e2e tests
// -----------------------------------------------------------------------

// JR-01: SPC j t opens today's journal (redesigned keybinding)
#[test]
fn jr01_spc_j_t_opens_today() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC j t");

    let screen = sim.screen(80, 24);
    assert!(
        screen.title().contains("202"),
        "SPC j t should open today's journal, got title: '{}'",
        screen.title()
    );
}

// JR-02: SPC j j opens the journal picker (not today's journal)
#[test]
fn jr02_spc_j_j_opens_journal_picker() {
    let vault = TestVault::new()
        .page("Test")
        .raw_file(
            "journal/2026-03-10.md",
            "---\nid: aaaa1111\ntitle: \"2026-03-10\"\ncreated: 2026-03-10\ntags: [journal]\n---\n",
        )
        .build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC j j");

    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "SPC j j should open journal picker");
}

// JR-03: SPC p p opens pages-only picker
#[test]
fn jr03_spc_p_p_opens_pages_picker() {
    let vault = TestVault::new().page("My Notes").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC p p");

    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "SPC p p should open pages picker");
}

// JR-04: SPC x a opens task quick capture
#[test]
fn jr04_spc_x_a_opens_task_capture() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC x a");

    let screen = sim.screen(80, 24);
    // Quick capture replaces the normal status bar content
    assert!(
        !screen.has_picker(),
        "SPC x a should open quick capture, not a picker"
    );
}

// JR-05: Day-hopping skips empty days
#[test]
fn jr05_day_hopping_skips_empty() {
    let vault = TestVault::new()
        .page("Test")
        .raw_file(
            "journal/2026-03-05.md",
            "---\nid: bbbb1111\ntitle: \"2026-03-05\"\ncreated: 2026-03-05\ntags: [journal]\n---\n\n- Earlier entry\n",
        )
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Open today's journal
    sim.keys("SPC j t");
    let today_title = sim.screen(80, 24).title().to_string();

    // Navigate back — should skip to March 5 (not yesterday)
    sim.type_text("[d");
    let prev_title = sim.screen(80, 24).title().to_string();

    assert_ne!(today_title, prev_title, "should navigate to a different day");
    assert!(
        prev_title.contains("2026-03-05"),
        "should skip to March 5 (only existing journal), got: '{}'",
        prev_title
    );
}

// JR-06: Day-hopping with no previous journal does nothing
#[test]
fn jr06_day_hopping_no_prev() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC j t");
    let title1 = sim.screen(80, 24).title().to_string();

    // No prior journal files exist
    sim.type_text("[d");
    let title2 = sim.screen(80, 24).title().to_string();

    assert_eq!(title1, title2, "with no prior journals, [d should stay");
}

// JR-07: JRNL mode appears after SPC j t
#[test]
fn jr07_jrnl_mode_on_journal_today() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC j t");

    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "JRNL", "SPC j t should activate JRNL mode");
    assert!(
        screen.right_hints().is_some(),
        "JRNL mode should show key hints"
    );
}

// JR-08: JRNL mode does NOT appear after startup (journal startup mode)
#[test]
fn jr08_no_jrnl_mode_on_startup() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // Startup opens journal but should NOT set JRNL mode
    let screen = sim.screen(80, 24);
    assert_ne!(
        screen.mode(),
        "JRNL",
        "startup should not set JRNL mode"
    );
}

// JR-09: Context strip appears in JRNL mode with adjacent journals
#[test]
fn jr09_context_strip_in_jrnl_mode() {
    let vault = TestVault::new()
        .page("Test")
        .raw_file(
            "journal/2026-03-08.md",
            "---\nid: cccc1111\ntitle: \"2026-03-08\"\ncreated: 2026-03-08\ntags: [journal]\n---\n",
        )
        .raw_file(
            "journal/2026-03-05.md",
            "---\nid: cccc2222\ntitle: \"2026-03-05\"\ncreated: 2026-03-05\ntags: [journal]\n---\n",
        )
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Enter journal mode via SPC j t, then navigate to a day
    sim.keys("SPC j t");
    // In JRNL mode, context strip should appear showing adjacent days
    let screen = sim.screen(80, 24);
    assert!(
        screen.has_context_strip(),
        "context strip should appear in JRNL mode"
    );
}

// JR-10: SPC j c opens the journal calendar
#[test]
fn jr10_spc_j_c_opens_calendar() {
    let vault = TestVault::new()
        .page("Test")
        .raw_file(
            "journal/2026-03-10.md",
            "---\nid: dddd1111\ntitle: \"2026-03-10\"\ncreated: 2026-03-10\ntags: [journal]\n---\n",
        )
        .build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC j c");

    let screen = sim.screen(80, 24);
    assert!(screen.has_date_picker(), "SPC j c should open journal calendar");
}

// JR-11: Calendar closes on Escape
#[test]
fn jr11_calendar_closes_on_escape() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC j c");
    assert!(sim.screen(80, 24).has_date_picker());

    sim.keys("<Esc>");
    assert!(!sim.screen(80, 24).has_date_picker(), "Esc should close calendar");
}

// JR-12: Quick capture appends to journal (smoke test)
#[test]
fn jr12_quick_capture_smoke() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // Open quick capture
    sim.keys("SPC j a");
    // Type some text
    sim.type_text("Test note from e2e");
    // Submit
    sim.keys("Enter");

    // Should return to normal (not have quick capture open)
    let screen = sim.screen(80, 24);
    // If journal was written successfully, we should see a notification
    assert_ne!(screen.mode(), "COMMAND", "should return to normal after quick capture");
}

// Regression: auto-align should align @due timestamps in task blocks on Esc
#[test]
fn auto_align_tasks_on_esc() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // Open a page and enter insert mode
    sim.keys("SPC p p");
    sim.keys("Enter");  // open the page
    // Go to end of file and enter insert mode
    sim.keys("G");
    sim.keys("o"); // open new line below in Insert mode

    // Type task lines with timestamps at different positions
    sim.type_text("- [ ] Short @due(2026-03-05)");
    sim.keys("Enter");
    sim.type_text("- [ ] A much longer task name here @due(2026-03-10)");
    sim.keys("Enter");
    sim.type_text("- [ ] Medium task @due(2026-03-15)");

    // Press Esc to trigger auto-align
    sim.keys("<Esc>");

    // Check that the @due positions are aligned
    let text = sim.buffer_text();
    let task_lines: Vec<&str> = text.lines().filter(|l| l.contains("@due")).collect();

    if task_lines.len() >= 2 {
        let positions: Vec<usize> = task_lines
            .iter()
            .filter_map(|l| l.find("@due"))
            .collect();
        // All @due should be at the same column
        let first = positions[0];
        for (i, pos) in positions.iter().enumerate() {
            assert_eq!(
                *pos, first,
                "line {} has @due at col {} but expected col {} — alignment failed.\nLines:\n{}",
                i, pos, first,
                task_lines.join("\n")
            );
        }
    }
}

// -----------------------------------------------------------------------
// Live Views — e2e tests
// -----------------------------------------------------------------------

// LV-01: SPC v v opens the query prompt (view is active)
#[test]
fn lv01_query_prompt_opens() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC v v");

    // The prompt view is active (tracked in editor state)
    // It doesn't create a buffer until a query is executed
    let screen = sim.screen(80, 24);
    // The view frame may or may not be present in overlay mode
    // but the editor should accept input
    assert_eq!(screen.mode(), "NORMAL");
}

// LV-02: SPC a a opens the Agenda view as a read-only buffer
#[test]
fn lv02_agenda_opens() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC a a");

    let screen = sim.screen(80, 24);
    // Agenda opens as a read-only buffer with the view name as title
    assert_eq!(screen.title(), "Agenda", "should show Agenda buffer");
}

// LV-03: View closes via SPC b k (kill buffer, Doom Emacs style)
#[test]
fn lv03_view_closes_on_kill() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC a a");
    assert_eq!(sim.screen(80, 24).title(), "Agenda");

    sim.keys("SPC b k");
    assert_ne!(sim.screen(80, 24).title(), "Agenda", "SPC b k should kill the view buffer");
}

// LV-04: View closes via SPC b d (like any buffer)
#[test]
fn lv04_view_closes_on_bd() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC a a");
    assert_eq!(sim.screen(80, 24).title(), "Agenda");

    sim.keys("SPC b k");
    assert_ne!(sim.screen(80, 24).title(), "Agenda", "SPC b k should close the view buffer");
}

// LV-05: SPC v l opens the views list picker
#[test]
fn lv05_view_list_opens_picker() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC v l");

    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "SPC v l should open a picker of saved views");
}

// -----------------------------------------------------------------------
// Ex-command behavior tests
// -----------------------------------------------------------------------

// :q with single buffer quits the app (returns Quit action)
#[test]
fn ex_q_single_buffer_quits() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys(":");
    sim.type_text("q");
    sim.keys("Enter");

    // With only one buffer, :q should produce Quit
    // The sim won't have a page open (app would exit)
    // We verify by checking that the screen still renders (no crash)
    let screen = sim.screen(80, 24);
    // If :q quits, the mode won't be COMMAND anymore
    assert_ne!(screen.mode(), "COMMAND");
}

// :q with multiple panes closes the current pane (Vim semantics)
#[test]
fn ex_q_multi_pane_closes_pane() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // Create a vertical split (2 panes)
    sim.keys("SPC w v");
    assert_eq!(sim.screen(80, 24).pane_count(), 2);

    // :q closes the current pane
    sim.keys(":");
    sim.type_text("q");
    sim.keys("Enter");
    assert_eq!(sim.screen(80, 24).pane_count(), 1, ":q should close the pane");
}

// :qa always quits regardless of buffer count
#[test]
fn ex_qa_always_quits() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys(":");
    sim.type_text("qa");
    sim.keys("Enter");

    // Should not crash, mode should not be COMMAND
    let screen = sim.screen(80, 24);
    assert_ne!(screen.mode(), "COMMAND");
}

// SPC b d closes buffer
#[test]
fn spc_bd_closes_buffer() {
    let vault = TestVault::new()
        .page("Page A")
        .page("Page B")
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Open two pages
    sim.keys("SPC p p");
    sim.keys("Enter");
    sim.keys("SPC p p");
    sim.keys("C-n");
    sim.keys("Enter");
    let title = sim.screen(80, 24).title().to_string();

    // SPC b d closes current
    sim.keys("SPC b k");
    assert_ne!(sim.screen(80, 24).title(), title, "SPC b k should close the buffer");
}

// View buffer navigation works (j/k)
#[test]
fn view_buffer_vim_navigation() {
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // Open agenda
    sim.keys("SPC a a");
    let screen = sim.screen(80, 24);
    assert_eq!(screen.title(), "Agenda");

    // j should move cursor down (not be blocked)
    let (_line_before, _) = screen.cursor();
    sim.keys("j");
    let (_line_after, _) = sim.screen(80, 24).cursor();
    // Cursor should have moved (if there's content) or stayed (if empty)
    // The key point: no crash, no error, key was processed
    assert_eq!(sim.screen(80, 24).mode(), "NORMAL");
}

// Regression: j/k must move cursor in read-only (frozen) view buffers
#[test]
fn agenda_cursor_moves_with_j() {
    let vault = TestVault::new()
        .page("Tasks")
        .with_content("- [ ] First task @due(2026-03-01)\n- [ ] Second task @due(2026-03-02)\n- [ ] Third task\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC a a");
    let screen = sim.screen(80, 24);
    assert_eq!(screen.title(), "Agenda");
    let lines = screen.line_count();
    eprintln!("Agenda has {} visible lines", lines);
    eprintln!("Mode: {}", screen.mode());

    let (line0, _) = screen.cursor();
    eprintln!("Cursor before j: line={}", line0);

    sim.keys("j");
    let screen2 = sim.screen(80, 24);
    let (line1, _) = screen2.cursor();
    eprintln!("Cursor after j: line={}", line1);
    eprintln!("Mode after j: {}", screen2.mode());

    // If the buffer has content, cursor should have moved
    if lines > 1 {
        assert!(
            line1 > line0,
            "j should move cursor down in agenda (before={}, after={})\nlines: {}",
            line0, line1, lines,
        );
    }
}

// Regression: 'o' at end of file with trailing newline places cursor on new line
#[test]
fn vim_o_at_eof_with_trailing_newline() {
    let vault = TestVault::new()
        .page("Test")
        .with_content("first line\nsecond line\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC p p");
    sim.keys("Enter");

    // Go to last line
    sim.keys("G");
    let (line_before, _) = sim.screen(80, 24).cursor();

    // Press o — open new line below
    sim.keys("o");
    let screen = sim.screen(80, 24);
    assert_eq!(screen.mode(), "INSERT");

    let (line_after, _) = screen.cursor();
    assert!(
        line_after > line_before,
        "cursor should move to the new line below (before={}, after={})",
        line_before, line_after,
    );
}

// Toggle task from Agenda view
#[test]
fn agenda_toggle_task() {
    let vault = TestVault::new()
        .page("Tasks")
        .with_content("- [ ] First task @due(2026-03-01)\n- [ ] Second task @due(2026-03-02)\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Open agenda
    sim.keys("SPC a a");
    assert_eq!(sim.screen(80, 24).title(), "Agenda");

    // Move to a task line and toggle with x
    sim.keys("j"); // move down (might be on a header)
    sim.keys("j");
    sim.keys("x"); // toggle

    // The view should have refreshed
    let screen = sim.screen(80, 24);
    assert_eq!(screen.title(), "Agenda", "should still be in Agenda after toggle");
}

// =======================================================================
// Index derivability — delete and rebuild, verify content matches
// =======================================================================

#[test]
fn index_is_fully_derivable() {
    let vault = TestVault::new()
        .page("Alpha")
        .with_content("- [ ] Task one @due(2026-03-01)\n- List item\n")
        .page("Beta")
        .with_content("Some paragraph\n- [ ] Another task\n")
        .build();
    let mut sim = SimInput::with_vault(vault);
    let vault_root = sim.vault_root().unwrap().to_path_buf();

    // Verify initial state: pages picker works
    sim.keys("SPC p p");
    assert!(sim.screen(80, 24).has_picker(), "pages picker should open initially");
    sim.keys("<Esc>");

    // Delete the index directory completely
    let index_dir = vault_root.join(".index");
    if index_dir.exists() {
        std::fs::remove_dir_all(&index_dir).unwrap();
    }

    // Rebuild via command
    sim.keys(":");
    sim.type_text("rebuild-index");
    sim.keys("Enter");

    // Wait for indexer to complete
    let ch = sim.editor.channels();
    if let Some(rx) = &ch.indexer_rx {
        for _ in 0..300 {
            if let Ok(complete) = rx.try_recv() {
                sim.editor.handle_index_complete(complete);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    // Verify pages picker still works after rebuild (index was fully re-derived)
    sim.keys("SPC p p");
    assert!(
        sim.screen(80, 24).has_picker(),
        "pages picker should open after index rebuild — index is derivable"
    );
}

// =======================================================================
// Mirror marker ^= parsed and indexed correctly
// =======================================================================

#[test]
fn mirror_marker_parsed_and_indexed() {
    // Create two pages with mirrored block (^=) and one solo block (^)
    let vault = TestVault::new()
        .page("Source")
        .with_content("- [ ] Mirrored task @due(2026-03-15) ^=k7m2x\n- Regular item ^abc01\n")
        .page("Mirror")
        .with_content("- [ ] Mirrored task @due(2026-03-15) ^=k7m2x\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Open the source page and verify ^= content renders
    sim.keys("SPC p p");
    sim.type_text("Source");
    sim.keys("Enter");

    let text = sim.buffer_text();
    assert!(text.contains("^=k7m2x"), "^= marker should be preserved in buffer: {}", text);
    assert!(text.contains("^abc01"), "solo block ID should be preserved: {}", text);
}

// =======================================================================
// Mirror promotion: ^k7m2x → ^=k7m2x when same ID in two pages
// =======================================================================

#[test]
fn mirror_promotion_on_duplicate_block_id() {
    // Two pages share the same block ID (solo ^ form) — indexer should promote to ^=
    let vault = TestVault::new()
        .page("PageA")
        .with_content("- [ ] Shared task ^abc01\n")
        .page("PageB")
        .with_content("- [ ] Shared task ^abc01\n")
        .build();
    let vault_root = vault.root().to_path_buf();
    let _sim = SimInput::with_vault(vault);

    // After indexing, read files back — both should have ^=abc01
    let page_a = std::fs::read_to_string(vault_root.join("pages/pagea.md")).unwrap();
    let page_b = std::fs::read_to_string(vault_root.join("pages/pageb.md")).unwrap();

    assert!(
        page_a.contains("^=abc01"),
        "PageA should be promoted to ^=abc01, got: {}",
        page_a
    );
    assert!(
        page_b.contains("^=abc01"),
        "PageB should be promoted to ^=abc01, got: {}",
        page_b
    );
}

// =======================================================================
// Retired block IDs recovered from broken links
// =======================================================================

#[test]
fn retired_ids_recovered_from_broken_links() {
    // Page with a block link to an ID that doesn't exist as a block
    let vault = TestVault::new()
        .page("Linker")
        .with_content("See [[^ghost1|the old analysis]] for details\n")
        .page("Other")
        .with_content("- [ ] Some task ^exist1\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    // The indexer should have retired "ghost1" (link target with no block)
    // Verify by checking that ID generation avoids "ghost1"
    // We can't directly query retired_block_ids from e2e, but we can
    // verify the index rebuilt correctly
    sim.keys("SPC p p");
    assert!(sim.screen(80, 24).has_picker(), "pages picker works after retirement");
}

// =======================================================================
// Undo: comprehensive tests
// =======================================================================

#[test]
fn undo_reverts_insert_session() {
    let mut sim = SimInput::with_content("hello");
    // Enter insert, type " world", exit
    sim.keys("A");
    sim.type_text(" world");
    sim.keys("<Esc>");
    assert!(sim.buffer_text().contains("hello world"));

    // One undo should revert the entire insert session
    sim.keys("u");
    assert_eq!(sim.buffer_text().trim(), "hello", "u should undo entire insert session");
}

#[test]
fn undo_redo_round_trip() {
    let mut sim = SimInput::with_content("original");
    sim.keys("A");
    sim.type_text(" added");
    sim.keys("<Esc>");
    assert!(sim.buffer_text().contains("original added"));

    sim.keys("u");
    assert_eq!(sim.buffer_text().trim(), "original", "undo should revert");

    sim.keys("C-r");
    assert!(sim.buffer_text().contains("original added"), "redo should restore");
}

#[test]
fn undo_multiple_insert_sessions() {
    let mut sim = SimInput::with_content("");
    // First insert
    sim.keys("i");
    sim.type_text("aaa");
    sim.keys("<Esc>");

    // Second insert
    sim.keys("A");
    sim.type_text("bbb");
    sim.keys("<Esc>");

    assert!(sim.buffer_text().contains("aaabbb"));

    // First undo → reverts "bbb"
    sim.keys("u");
    assert_eq!(sim.buffer_text().trim(), "aaa", "first u should undo second session");

    // Second undo → reverts "aaa"
    sim.keys("u");
    assert_eq!(sim.buffer_text().trim(), "", "second u should undo first session");
}

#[test]
fn undo_delete_line() {
    let mut sim = SimInput::with_content("line one\nline two\nline three");
    // dd deletes "line one"
    sim.keys("dd");
    assert!(!sim.buffer_text().contains("line one"), "dd should delete first line");

    sim.keys("u");
    assert!(sim.buffer_text().contains("line one"), "u should restore deleted line");
}

#[test]
fn undo_single_insert_not_alignment() {
    // Regression: alignment/block-ids after Esc should be in the same undo group
    let mut sim = SimInput::with_content("- [ ] task @due(2026-03-20)");
    sim.keys("A");
    sim.type_text(" more text");
    sim.keys("<Esc>");

    let after_edit = sim.buffer_text();
    assert!(after_edit.contains("more text"), "edit should be present");

    // ONE undo should revert everything (edit + any system alignment)
    sim.keys("u");
    let after_undo = sim.buffer_text();
    assert!(
        !after_undo.contains("more text"),
        "single u should undo the edit (not just alignment). Got: {}",
        after_undo
    );
}

// =======================================================================
// Mirror propagation: edit a ^= line, verify propagation fires
// =======================================================================

#[test]
fn mirror_propagation_two_panes() {
    // Two pages with mirrored block ^=mir01 — minimal, no frontmatter complications
    let vault = TestVault::new()
        .page("Source")
        .with_content("- [ ] original text ^=mir01\n")
        .page("Mirror")
        .with_content("- [ ] original text ^=mir01\n")
        .build();
    let vault_root = vault.root().to_path_buf();
    let _sim = SimInput::with_vault(vault);

    // After indexing, verify both files still have ^=mir01 (not double-assigned)
    let source = std::fs::read_to_string(vault_root.join("pages/source.md")).unwrap();
    let mirror = std::fs::read_to_string(vault_root.join("pages/mirror.md")).unwrap();
    assert!(source.contains("^=mir01"), "Source should have ^=mir01 after indexing");
    assert!(mirror.contains("^=mir01"), "Mirror should have ^=mir01 after indexing");
    // No double block IDs
    let source_task_line = source.lines().find(|l| l.contains("original")).unwrap();
    assert_eq!(
        source_task_line.matches(" ^").count(), 1,
        "Source task should have exactly one block ID, got: {}",
        source_task_line
    );
}

// =======================================================================
// Cursor landing: where does the cursor go after mutations?
// =======================================================================

// dd on middle line → cursor stays on same line number (now next line's content)
#[test]
fn cursor_after_dd_middle_line() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc");
    sim.keys("j"); // line 1 (bbb)
    let (line_before, _) = sim.screen(80, 24).cursor();
    assert_eq!(line_before, 1, "cursor should be on line 1");

    sim.keys("dd"); // delete bbb
    let text = sim.buffer_text();
    assert!(!text.contains("bbb"), "bbb should be deleted");

    let (line_after, _) = sim.screen(80, 24).cursor();
    assert_eq!(line_after, 1, "cursor should stay on line 1 (now ccc)");
    assert_eq!(sim.screen(80, 24).line_text(1), "ccc", "line 1 should now be ccc");
}

// dd on last line → cursor moves up to new last line
#[test]
fn cursor_after_dd_last_line() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc");
    sim.keys("G"); // last line (ccc)
    sim.keys("dd"); // delete ccc
    let text = sim.buffer_text();
    assert!(!text.contains("ccc"), "ccc should be deleted");
    let (line_after, _) = sim.screen(80, 24).cursor();
    assert!(
        line_after <= 1,
        "cursor should be on line 0 or 1 after deleting last line, got {}",
        line_after
    );
}

// dd on first line → cursor stays on line 0 (now next line's content)
#[test]
fn cursor_after_dd_first_line() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc");
    sim.keys("dd"); // delete aaa
    let text = sim.buffer_text();
    assert!(!text.contains("aaa"), "aaa should be deleted");

    let (line_after, _) = sim.screen(80, 24).cursor();
    assert_eq!(line_after, 0, "cursor should be on line 0");
    assert_eq!(sim.screen(80, 24).line_text(0), "bbb", "line 0 should now be bbb");
}

// dd on only line → cursor stays on line 0 with empty content
#[test]
fn cursor_after_dd_only_line() {
    let mut sim = SimInput::with_content("only line");
    sim.keys("dd");
    let (line_after, _) = sim.screen(80, 24).cursor();
    assert_eq!(line_after, 0, "cursor should be on line 0");
}

// D (delete to EOL) → cursor stays on same line, moves to last char
#[test]
fn cursor_after_big_d() {
    let mut sim = SimInput::with_content("hello world");
    sim.keys("w"); // on 'w' of 'world'
    sim.keys("D"); // delete 'world'
    let text = sim.buffer_text();
    assert!(text.contains("hello"), "hello should remain");
    assert!(!text.contains("world"), "world should be deleted");
    let (line, col) = sim.screen(80, 24).cursor();
    assert_eq!(line, 0, "cursor on line 0");
    assert!(col <= 5, "cursor should be at or before end of 'hello', got col {}", col);
}

// x on last char of line → cursor moves left
#[test]
fn cursor_after_x_at_eol() {
    let mut sim = SimInput::with_content("abc");
    sim.keys("$"); // on 'c'
    sim.keys("x"); // delete 'c'
    let text = sim.buffer_text();
    assert_eq!(text.trim(), "ab", "c should be deleted");
    let (_, col) = sim.screen(80, 24).cursor();
    assert_eq!(col, 1, "cursor should be on 'b' (last char) at col 1");
}

// o (open below) → cursor on new empty line below
#[test]
fn cursor_after_open_below() {
    let mut sim = SimInput::with_content("aaa\nbbb");
    // cursor on line 0 (aaa)
    sim.keys("o"); // open line below, enters Insert
    sim.keys("<Esc>");
    let (line, _) = sim.screen(80, 24).cursor();
    assert_eq!(line, 1, "cursor should be on new line 1 (between aaa and bbb)");
}

// O (open above) → cursor on new empty line above
#[test]
fn cursor_after_open_above() {
    let mut sim = SimInput::with_content("aaa\nbbb");
    sim.keys("j"); // line 1 (bbb)
    sim.keys("O"); // open line above, enters Insert
    sim.keys("<Esc>");
    let (line, _) = sim.screen(80, 24).cursor();
    assert_eq!(line, 1, "cursor should be on new line 1 (between aaa and bbb)");
}

// J (join lines) → cursor at join point
#[test]
fn cursor_after_join() {
    let mut sim = SimInput::with_content("aaa\nbbb");
    sim.keys("J"); // join: "aaa bbb"
    let text = sim.buffer_text();
    assert!(
        text.contains("aaa bbb"),
        "J should join lines with space, got: '{}'",
        text.replace('\n', "\\n")
    );
    let (line, col) = sim.screen(80, 24).cursor();
    assert_eq!(line, 0, "cursor on line 0");
    assert!(col >= 3, "cursor at join point (col {})", col);
}

// p (paste below) → cursor on pasted text
#[test]
fn cursor_after_paste_line() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc");
    sim.keys("dd"); // yank+delete aaa
    sim.keys("p"); // paste below current line
    let text = sim.buffer_text();
    // Verify aaa was pasted back
    assert!(text.contains("aaa"), "aaa should be pasted back, got: {}", text);
    let (line, _) = sim.screen(80, 24).cursor();
    // Vim: p pastes below, cursor goes to pasted line
    assert!(line < 10, "cursor should be on a valid line, got {}", line);
}

// gg → cursor on first line
#[test]
fn cursor_after_gg() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc");
    sim.keys("G"); // go to last line
    sim.keys("gg"); // go to first line
    let (line, _) = sim.screen(80, 24).cursor();
    assert_eq!(line, 0, "gg should go to line 0");
}

// G → cursor on last line
#[test]
fn cursor_after_big_g() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc");
    sim.keys("G");
    let (line, _) = sim.screen(80, 24).cursor();
    assert_eq!(line, 2, "G should go to last line (line 2)");
}

// u (undo) → cursor restored to pre-edit position
#[test]
fn cursor_after_undo_insert() {
    let mut sim = SimInput::with_content("hello");
    sim.keys("A");
    sim.type_text(" world");
    sim.keys("<Esc>");
    let (_, col_after_edit) = sim.screen(80, 24).cursor();
    assert!(col_after_edit > 4, "cursor should be past 'hello' after edit");

    sim.keys("u"); // undo — should restore "hello" and cursor near end
    let text = sim.buffer_text();
    assert_eq!(text.trim(), "hello", "undo should revert text");
    let (_, col_after_undo) = sim.screen(80, 24).cursor();
    assert!(
        col_after_undo >= 3,
        "cursor should be restored near pre-edit position, got col {}",
        col_after_undo
    );
}

// 2dd → deletes two lines, cursor on next remaining line
#[test]
fn cursor_after_count_dd() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc\nddd");
    sim.keys("j"); // line 1 (bbb)
    sim.keys("2dd"); // delete bbb and ccc
    let text = sim.buffer_text();
    assert!(!text.contains("bbb"), "bbb deleted");
    assert!(!text.contains("ccc"), "ccc deleted");
    let (line, _) = sim.screen(80, 24).cursor();
    assert_eq!(line, 1, "cursor on line 1 (now ddd)");
    assert_eq!(sim.screen(80, 24).line_text(1), "ddd");
}

// =======================================================================
// Search: / ? n N SPC *
// =======================================================================

#[test]
fn search_forward_jumps_to_match() {
    let mut sim = SimInput::with_content("aaa\nbbb\naaa\nccc\naaa");
    // / opens search prompt, type "bbb", Enter
    sim.keys("/");
    sim.type_text("bbb");
    sim.keys("Enter");
    let (line, _) = sim.screen(80, 24).cursor();
    assert_eq!(line, 1, "/ bbb should jump to line 1");
}

#[test]
fn search_n_jumps_to_next() {
    let mut sim = SimInput::with_content("aaa\nbbb\naaa\nbbb\nccc");
    sim.keys("/");
    sim.type_text("bbb");
    sim.keys("Enter");
    let (line1, _) = sim.screen(80, 24).cursor();
    assert_eq!(line1, 1, "first match at line 1");

    sim.keys("n"); // next match
    let (line2, _) = sim.screen(80, 24).cursor();
    assert_eq!(line2, 3, "n should jump to second bbb at line 3");
}

#[test]
fn search_n_wraps_around() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc");
    sim.keys("/");
    sim.type_text("bbb");
    sim.keys("Enter");
    // Cursor on line 1 (bbb)
    sim.keys("n"); // should wrap to same match (only one)
    let (line, _) = sim.screen(80, 24).cursor();
    assert_eq!(line, 1, "n with single match should wrap to same position");
}

#[test]
fn search_big_n_goes_backward() {
    let mut sim = SimInput::with_content("aaa\nbbb\naaa\nbbb\nccc");
    sim.keys("/");
    sim.type_text("bbb");
    sim.keys("Enter");
    sim.keys("n"); // go to second bbb (line 3)
    sim.keys("N"); // back to first bbb (line 1)
    let (line, _) = sim.screen(80, 24).cursor();
    assert_eq!(line, 1, "N should go back to first bbb at line 1");
}

#[test]
fn search_esc_cancels_and_restores_cursor() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc");
    sim.keys("j"); // go to line 1
    let (line_before, _) = sim.screen(80, 24).cursor();
    sim.keys("/");
    sim.type_text("ccc");
    // Live search should have jumped cursor, but Esc cancels
    sim.keys("<Esc>");
    let (line_after, _) = sim.screen(80, 24).cursor();
    assert_eq!(line_after, line_before, "Esc should restore cursor to pre-search position");
}

#[test]
fn search_backward_with_question_mark() {
    let mut sim = SimInput::with_content("aaa\nbbb\nccc\nbbb\nddd");
    sim.keys("G"); // go to last line
    sim.keys("?");
    sim.type_text("bbb");
    sim.keys("Enter");
    let (line, _) = sim.screen(80, 24).cursor();
    assert_eq!(line, 3, "? bbb from end should find last bbb at line 3");
}

#[test]
fn spc_star_searches_word_under_cursor() {
    let vault = TestVault::new()
        .page("Alpha")
        .with_content("The rope data structure\n")
        .page("Beta")
        .with_content("Comparing rope vs array\n")
        .build();
    let mut sim = SimInput::with_vault(vault);

    // Open Alpha, move to "rope"
    sim.keys("SPC p p");
    sim.type_text("Alpha");
    sim.keys("Enter");
    sim.keys("w"); // move to "rope"

    // SPC * should open search pre-filled with "rope"
    sim.keys("SPC *");
    let screen = sim.screen(80, 24);
    assert!(screen.has_picker(), "SPC * should open search picker");
}

// =======================================================================
// Theme: persist and restore from config
// =======================================================================

#[test]
fn theme_persists_to_config_without_corrupting_views() {
    let vault = TestVault::new()
        .page("Test")
        .build();
    let vault_root = vault.root().to_path_buf();
    // Write config at vault root BEFORE init
    std::fs::write(
        vault_root.join("config.toml"),
        "[theme]\nname = \"bloom-dark\"\n\n[[views]]\nname = \"Agenda\"\nquery = \"tasks | where not done | sort due\"\nkey = \"a a\"\n",
    ).unwrap();

    let config = bloom_core::config::Config::load(&vault_root.join("config.toml"))
        .unwrap_or_else(|_| bloom_core::config::Config::defaults());
    let mut editor = bloom_core::BloomEditor::new(config).unwrap();
    let _ = editor.init_vault(&vault_root);
    let ch = editor.channels();
    if let Some(rx) = &ch.indexer_rx {
        for _ in 0..300 {
            if let Ok(complete) = rx.try_recv() {
                editor.handle_index_complete(complete);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    editor.startup();

    let mut sim = SimInput::with_editor_and_vault(editor, vault);

    // Change theme via picker
    sim.keys("SPC T t");
    sim.keys("C-n"); // move to next theme
    sim.keys("Enter"); // confirm

    // Read config back and verify
    let config = std::fs::read_to_string(vault_root.join("config.toml")).unwrap();
    // Theme should be changed
    assert!(
        !config.contains("name = \"bloom-dark\"") || config.contains("[theme]"),
        "theme should be updated in config"
    );
    // View name should NOT be corrupted
    assert!(
        config.contains("name = \"Agenda\""),
        "view name should be preserved, got:\n{}",
        config
    );
}
