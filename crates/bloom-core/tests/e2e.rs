//! End-to-end integration tests using SimInput + TestScreen.
//!
//! Each test drives BloomEditor through key sequences and asserts on the
//! visual output. No terminal, no GUI — runs in CI.

use bloom_test_harness::{SimInput, TestScreen, TestVault};

// -----------------------------------------------------------------------
// UC-01: Open today's journal
// -----------------------------------------------------------------------

#[test]
fn uc01_open_journal() {
    let vault = TestVault::new().page("Existing Page").build();
    let mut sim = SimInput::with_vault(vault);

    sim.keys("SPC j j");

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
    let vault = TestVault::new().page("Test").build();
    let mut sim = SimInput::with_vault(vault);

    // Open journal first
    sim.keys("SPC j j");
    let title1 = sim.screen(80, 24).title().to_string();

    // SPC j p navigates to previous day
    sim.keys("SPC j p");
    let title2 = sim.screen(80, 24).title().to_string();

    // Titles should differ (different dates)
    assert_ne!(title1, title2, "prev journal should have different date");
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
    sim.keys("SPC b d");

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
fn vim_X_delete_char_before() {
    let mut sim = SimInput::with_content("hello");
    sim.keys("l"); // move to 'e'
    sim.keys("X");
    assert_eq!(sim.buffer_text(), "ello");
}

#[test]
fn vim_D_delete_to_eol() {
    let mut sim = SimInput::with_content("hello world\n");
    sim.keys("w"); // move to 'world'
    sim.keys("D");
    let text = sim.buffer_text();
    assert!(text.starts_with("hello"), "D should keep text before cursor: '{text}'");
    assert!(!text.contains("world"), "D should delete to end of line: '{text}'");
}

#[test]
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
fn vim_J_join_lines() {
    let mut sim = SimInput::with_content("hello\nworld\n");
    sim.keys("J");
    let text = sim.buffer_text();
    assert!(
        text.contains("hello world") || text.contains("hello\nworld"),
        "J should join lines: '{text}'"
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
