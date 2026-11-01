# M0-05 - Behavioral oracle inventory

> Baseline: `6d397be42070459c9470730a3cce8dbe94882ddc` (the selected v2 starting snapshot).
> This is an inventory of test cases, not a claim that the current legacy suite passes or that v2 parity exists.

## Scope and counts

- `crates/bloom-core/tests/e2e.rs`: 209 test cases, including ignored tests.
- `crates/bloom-vim/src/state.rs`: 42 Vim state/grammar cases.
- `crates/bloom-test-harness/tests/animations.rs`: 3 recorded user-journey scenarios.
- Additional tactical regressions: F1 deadline consumption and F13 vault-wide block-ID readiness/uniqueness.
- Total inventory entries: 256; IDs are unique source-path/function pairs. Ignored cases remain listed and are not parity passes.
- `uc95_block_id_assignment` is the existing e2e scenario strengthened by the tactical F13 work to open and save explicitly.

## Disposition rules

- **PORT** — preserve the named user outcome; implementation and harness may change.
- **REWRITE** — preserve the journey but replace its mechanism according to the cited accepted decision or regression invariant.
- **DROP** — remove only the listed implementation-specific assertion or a scenario whose behavior is explicitly removed; the reason names its replacement or explains why no product journey remains.
- Accepted product decisions are in [FIRST_PRINCIPLES_REDESIGN.md](FIRST_PRINCIPLES_REDESIGN.md), especially D2-D7. A new behavior change not authorized there is not implied by this inventory; escalate it to the human lead.

## Complete test roster

| Test ID | Baseline | Disposition | Target journey | Reason |
|---|---|---|---|---|
| `crates/bloom-core/tests/e2e.rs::uc01_open_journal` | active | **PORT** | UC01: open journal | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc08_find_page_opens_picker` | active | **PORT** | UC08: find page opens picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::reopening_existing_page_from_picker_preserves_cursor` | active | **PORT** | reopening existing page from picker preserves cursor | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc14_insert_mode_typing` | active | **PORT** | UC14: insert mode typing | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc14_jk_navigation` | active | **PORT** | UC14: jk navigation | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_does_not_wrap_to_line_zero` | active | **PORT** | cursor does not wrap to line zero | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_does_not_wrap_with_vault_page` | active | **PORT** | cursor does not wrap with vault page | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_does_not_go_past_eof_no_trailing_newline` | active | **PORT** | cursor does not go past eof no trailing newline | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_does_not_go_past_eof_single_line` | active | **PORT** | cursor does not go past eof single line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_past_eof_after_delete` | active | **PORT** | cursor past eof after delete | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc14_dw_delete_word` | active | **PORT** | UC14: dw delete word | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc15_count_motion` | active | **PORT** | UC15: count motion | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc15_d_dollar_deletes_to_eol` | active | **PORT** | UC15: d dollar deletes to eol | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc17_visual_select_and_delete` | active | **PORT** | UC17: visual select and delete | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc18_undo_redo` | active | **PORT** | UC18: undo redo | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc20_command_mode` | active | **PORT** | UC20: command mode | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc23_dot_repeat` | active | **PORT** | UC23: dot repeat | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc23_dot_repeat_insert_session` | active | **PORT** | UC23: dot repeat insert session | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc23_dot_repeat_x_multiple` | active | **PORT** | UC23: dot repeat x multiple | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc42_task_toggle_via_vim` | active | **PORT** | UC42: task toggle via vim | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc52_vertical_split` | active | **PORT** | UC52: vertical split | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc55_maximize_restore` | active | **PORT** | UC55: maximize restore | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc87_whichkey_shows_on_timeout` | active | **PORT** | UC87: whichkey shows on timeout | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::open_line_below` | active | **PORT** | open line below | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::open_line_above` | active | **PORT** | open line above | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::backspace_in_insert_mode` | active | **PORT** | backspace in insert mode | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::dirty_flag_set_on_edit` | active | **PORT** | dirty flag set on edit | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc02_quick_capture_journal_append` | active | **PORT** | UC02: quick capture journal append | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::dashboard_page_picker_is_visible` | active | **PORT** | dashboard page picker is visible | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::dashboard_search_picker_is_visible` | active | **PORT** | dashboard search picker is visible | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::dashboard_quick_capture_prompt_is_visible` | active | **PORT** | dashboard quick capture prompt is visible | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::dashboard_journal_today_opens_page` | active | **PORT** | dashboard journal today opens page | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc04_journal_prev_next` | active | **PORT** | UC04: journal prev next | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc11_switch_buffer` | active | **PORT** | UC11: switch buffer | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::switching_back_to_open_buffer_preserves_cursor` | active | **PORT** | switching back to open buffer preserves cursor | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc12_close_buffer` | active | **PORT** | UC12: close buffer | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc16_delete_inside_word` | active | **PORT** | UC16: delete inside word | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc16_change_inner_word` | active | **PORT** | UC16: change inner word | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc21_yank_and_paste` | active | **PORT** | UC21: yank and paste | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc21_dd_yank_line_and_paste` | active | **PORT** | UC21: dd yank line and paste | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc22_macro_record_replay` | active | **PORT** | UC22: macro record replay | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc37_search_opens_picker` | active | **PORT** | UC37: search opens picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc53_navigate_windows` | active | **PORT** | UC53: navigate windows | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc56_close_window` | active | **PORT** | UC56: close window | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_append_end_of_line` | active | **PORT** | vim append end of line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_append_end_of_line_before_hidden_block_id` | active | **PORT** | vim append end of line before hidden block id | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_append_end_of_line_before_hidden_mirror_block_id` | active | **DROP** | No distinct journey remains; hidden-ID editing is covered by vim_append_end_of_line_before_hidden_block_id. | D2 removes mirror syntax; this is a redundant mirror-token variant. |
| `crates/bloom-core/tests/e2e.rs::vim_change_word` | active | **PORT** | vim change word | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_replace_char` | active | **PORT** | vim replace char | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_goto_line_start_end` | active | **PORT** | vim goto line start end | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_gg_and_G` | active | **PORT** | vim gg and G | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_dG_deletes_through_last_line` | active | **PORT** | vim d G deletes through last line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_dG_undo_restores_firing_cursor` | active | **PORT** | vim d G undo restores firing cursor | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_w_b_word_motions` | active | **PORT** | vim w b word motions | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc89_all_commands_picker` | active | **PORT** | UC89: all commands picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc90_ctrl_s_saves` | active | **PORT** | UC90: ctrl s saves | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::enter_creates_newline_in_insert` | active | **PORT** | enter creates newline in insert | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::multiple_undo_redo` | active | **PORT** | multiple undo redo | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_x_delete_char` | active | **PORT** | vim x delete char | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_X_delete_char_before` | active | **PORT** | vim X delete char before | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_D_delete_to_eol` | active | **PORT** | vim D delete to eol | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_C_change_to_eol` | active | **PORT** | vim C change to eol | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_dd_delete_line` | active | **PORT** | vim dd delete line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_cc_change_line` | active | **PORT** | vim cc change line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_J_join_lines` | active | **PORT** | vim J join lines | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_yy_p_yank_paste_line` | active | **PORT** | vim yy p yank paste line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_count_dd` | active | **PORT** | vim count dd | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_f_find_char` | active | **PORT** | vim f find char | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_percent_matching_bracket` | active | **PORT** | vim percent matching bracket | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_visual_line_mode` | active | **PORT** | vim visual line mode | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc54_resize_window` | active | **PORT** | UC54: resize window | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc52_horizontal_split` | active | **PORT** | UC52: horizontal split | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc26_follow_link_opens_page` | active | **PORT** | UC26: follow link opens page | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc37_search_finds_content` | active | **PORT** | UC37: search finds content | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc34_search_tags` | active | **PORT** | UC34: search tags | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc76_rebuild_index_via_command` | active | **PORT** | UC76: rebuild index via command | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::ex_command_theme_switch` | active | **PORT** | ex command theme switch | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::command_mode_status_bar_stays_at_bottom` | active | **PORT** | command mode status bar stays at bottom | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::theme_picker_live_preview` | active | **PORT** | theme picker live preview | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::theme_picker_confirm_persists` | active | **PORT** | theme picker confirm persists | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::linked_vault_find_all_pages` | active | **PORT** | linked vault find all pages | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::linked_vault_open_specific_page` | active | **PORT** | linked vault open specific page | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::linked_vault_search_content` | active | **PORT** | linked vault search content | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::linked_vault_backlinks_picker` | active | **PORT** | linked vault backlinks picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::linked_vault_unlinked_mentions` | active | **PORT** | linked vault unlinked mentions | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::task_vault_find_pages` | active | **PORT** | task vault find pages | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::task_vault_open_project_a` | active | **PORT** | task vault open project a | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::task_vault_search_tasks` | active | **PORT** | task vault search tasks | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::task_vault_agenda_opens` | active | **PORT** | task vault agenda opens | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::task_vault_toggle_task` | active | **PORT** | task vault toggle task | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::tagged_vault_search_tags` | active | **PORT** | tagged vault search tags | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::tagged_vault_find_all_pages` | active | **PORT** | tagged vault find all pages | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::tagged_vault_open_meeting_notes` | active | **PORT** | tagged vault open meeting notes | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::open_two_pages_and_switch` | active | **PORT** | open two pages and switch | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::split_and_open_different_pages` | active | **PORT** | split and open different pages | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr01_spc_j_t_opens_today` | active | **PORT** | jr01 spc j t opens today | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr02_spc_j_j_opens_journal_picker` | active | **PORT** | jr02 spc j j opens journal picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr03_spc_p_p_opens_pages_picker` | active | **PORT** | jr03 spc p p opens pages picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr04_spc_x_a_opens_task_capture` | active | **PORT** | jr04 spc x a opens task capture | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr05_day_hopping_skips_empty` | active | **PORT** | jr05 day hopping skips empty | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr06_day_hopping_no_prev` | active | **PORT** | jr06 day hopping no prev | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr07_jrnl_mode_on_journal_today` | active | **PORT** | jr07 jrnl mode on journal today | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr08_jrnl_mode_on_journal_page` | active | **PORT** | jr08 jrnl mode on journal page | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr09_context_strip_in_jrnl_mode` | active | **PORT** | jr09 context strip in jrnl mode | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr10_spc_j_c_opens_calendar` | active | **PORT** | jr10 spc j c opens calendar | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr11_calendar_closes_on_escape` | active | **PORT** | jr11 calendar closes on escape | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::jr12_quick_capture_smoke` | active | **PORT** | jr12 quick capture smoke | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::auto_align_tasks_on_esc` | active | **PORT** | auto align tasks on esc | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::lv01_query_prompt_opens` | active | **PORT** | lv01 query prompt opens | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::lv02_agenda_opens` | active | **PORT** | lv02 agenda opens | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::lv03_view_closes_on_kill` | active | **PORT** | lv03 view closes on kill | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::lv04_view_closes_on_bd` | active | **PORT** | lv04 view closes on bd | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::lv05_view_list_opens_picker` | active | **PORT** | lv05 view list opens picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::ex_q_single_buffer_quits` | active | **PORT** | ex q single buffer quits | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::ex_q_multi_pane_closes_pane` | active | **PORT** | ex q multi pane closes pane | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::ex_qa_always_quits` | active | **PORT** | ex qa always quits | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::spc_bd_closes_buffer` | active | **PORT** | spc bd closes buffer | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::view_buffer_vim_navigation` | active | **PORT** | view buffer vim navigation | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::agenda_cursor_moves_with_j` | active | **PORT** | agenda cursor moves with j | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::vim_o_at_eof_with_trailing_newline` | active | **PORT** | vim o at eof with trailing newline | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::agenda_toggle_task` | active | **PORT** | agenda toggle task | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::index_is_fully_derivable` | active | **PORT** | index is fully derivable | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::mirror_marker_parsed_and_indexed` | active | **REWRITE** | Transclusion content is resolved and indexed while identity tokens stay hidden. | D2 replaces ^= mirrors with editable transclusion; preserve linked-content semantics. |
| `crates/bloom-core/tests/e2e.rs::mirror_promotion_on_duplicate_block_id` | active | **REWRITE** | Duplicate live IDs are rejected; IDs remain unique across pages. | D2/D6 and I4 replace implicit duplicate-ID promotion with vault-wide uniqueness. |
| `crates/bloom-core/tests/e2e.rs::retired_ids_recovered_from_broken_links` | active | **REWRITE** | Retired/aliased IDs from links remain reserved across rebuild and restart. | D7 requires retired IDs and aliases in the new durable state model. |
| `crates/bloom-core/tests/e2e.rs::undo_reverts_insert_session` | active | **PORT** | undo reverts insert session | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::undo_redo_round_trip` | active | **PORT** | undo redo round trip | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::undo_multiple_insert_sessions` | active | **PORT** | undo multiple insert sessions | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::undo_delete_line` | active | **PORT** | undo delete line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::undo_single_insert_not_alignment` | active | **PORT** | undo single insert not alignment | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::mirror_propagation_two_panes` | active | **REWRITE** | A transclusion source edit is reflected consistently in both panes. | D2 replaces automatic mirror propagation with explicit transclusion. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_dd_middle_line` | active | **PORT** | cursor after dd middle line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_dd_last_line` | active | **PORT** | cursor after dd last line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_dd_first_line` | active | **PORT** | cursor after dd first line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_dd_only_line` | active | **PORT** | cursor after dd only line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_big_d` | active | **PORT** | cursor after big d | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_x_at_eol` | active | **PORT** | cursor after x at eol | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_open_below` | active | **PORT** | cursor after open below | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_open_above` | active | **PORT** | cursor after open above | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_join` | active | **PORT** | cursor after join | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_paste_line` | active | **PORT** | cursor after paste line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_gg` | active | **PORT** | cursor after gg | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_big_g` | active | **PORT** | cursor after big g | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_undo_insert` | active | **PORT** | cursor after undo insert | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_undo_in_split_pane` | active | **PORT** | cursor after undo in split pane | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::cursor_after_count_dd` | active | **PORT** | cursor after count dd | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::search_forward_jumps_to_match` | active | **PORT** | search forward jumps to match | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::search_n_jumps_to_next` | active | **PORT** | search n jumps to next | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::search_n_wraps_around` | active | **PORT** | search n wraps around | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::search_big_n_goes_backward` | active | **PORT** | search big n goes backward | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::search_esc_cancels_and_restores_cursor` | active | **PORT** | search esc cancels and restores cursor | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::search_backward_with_question_mark` | active | **PORT** | search backward with question mark | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::spc_star_searches_word_under_cursor` | active | **PORT** | spc star searches word under cursor | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::theme_persists_to_config_without_corrupting_views` | active | **PORT** | theme persists to config without corrupting views | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::no_reload_dialog_after_self_save` | active | **PORT** | no reload dialog after self save | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::clean_reload_reanchors_cursor_by_line_and_column` | active | **PORT** | clean reload reanchors cursor by line and column | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::spc_j_t_does_not_create_duplicate_buffers` | active | **PORT** | spc j t does not create duplicate buffers | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::page_history_opens_with_hist_mode` | active | **PORT** | page history opens with hist mode | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::history_scrub_shows_undo_preview` | active | **PORT** | history scrub shows undo preview | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::history_restore_changes_buffer` | active | **PORT** | history restore changes buffer | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::block_history_opens_on_block_id` | active | **PORT** | block history opens on block id | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::block_history_not_on_line_without_id` | active | **PORT** | block history not on line without id | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::day_hopping_reuses_buffers` | active | **PORT** | day hopping reuses buffers | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::block_history_diff_contains_only_block_content` | active | **PORT** | block history diff contains only block content | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::block_history_diff_no_extra_content_in_long_file` | active | **PORT** | block history diff no extra content in long file | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::block_history_git_blob_extracts_block_line_only` | active | **REWRITE** | Block-history diff shows only the selected block, not unrelated page content. | D3 replaces Git blob storage with state.db; preserve the user-visible diff boundary. |
| `crates/bloom-core/tests/e2e.rs::block_history_git_blob_falls_back_to_line_number` | active | **REWRITE** | A historical block remains inspectable when its ID was absent in that version. | D3 replaces Git blob lookup; preserve historical block resolution without the legacy line-number adapter. |
| `crates/bloom-core/tests/e2e.rs::block_history_skips_unchanged_git_commits` | active | **REWRITE** | History navigation skips versions with no block-content change. | D3 replaces Git commit history with the version store; preserve the user-visible navigation result. |
| `crates/bloom-core/tests/e2e.rs::uc03_quick_capture_task_appends_to_todays_journal` | ignored | **PORT** | UC03: quick capture task appends to todays journal | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc05_jump_to_journal_by_date_opens_date_picker` | active | **PORT** | UC05: jump to journal by date opens date picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc07_create_page_from_template_with_frontmatter` | active | **PORT** | UC07: create page from template with frontmatter | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc09_rename_page_opens_input` | active | **PORT** | UC09: rename page opens input | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc09_rename_page_updates_title` | active | **PORT** | UC09: rename page updates title | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc10_delete_page_shows_confirmation_dialog` | active | **PORT** | UC10: delete page shows confirmation dialog | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc10_delete_page_cancel_keeps_page` | active | **PORT** | UC10: delete page cancel keeps page | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc10_delete_page_confirm_removes_page` | active | **PORT** | UC10: delete page confirm removes page | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc13_create_blank_page` | ignored | **PORT** | UC13: create blank page | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc19_history_surface_opens_from_spc_u_u` | active | **PORT** | UC19: history surface opens from spc u u | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc24_create_link_while_writing_shows_inline_menu` | active | **PORT** | UC24: create link while writing shows inline menu | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc25_create_link_to_nonexistent_page` | ignored | **PORT** | UC25: create link to nonexistent page | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc27_backlinks_picker` | active | **PORT** | UC27: backlinks picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc30_yank_link_to_clipboard_smoke` | active | **PORT** | UC30: yank link to clipboard smoke | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc32_add_tag_to_page` | ignored | **PORT** | UC32: add tag to page | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc33_remove_tag_opens_picker` | active | **PORT** | UC33: remove tag opens picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc35_inline_tag` | active | **PORT** | UC35: inline tag | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc36_tag_rename` | ignored | **PORT** | UC36: tag rename | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc38_search_with_filters_opens_search_picker` | active | **PORT** | UC38: search with filters opens search picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc39_search_journal_opens_picker` | active | **PORT** | UC39: search journal opens picker | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc41_create_a_task` | active | **PORT** | UC41: create a task | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc43_open_agenda` | active | **PORT** | UC43: open agenda | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc44_act_on_task_from_agenda` | active | **PORT** | UC44: act on task from agenda | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc46_filter_agenda_by_tag` | ignored | **PORT** | UC46: filter agenda by tag | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc47_insert_timestamp` | ignored | **PORT** | UC47: insert timestamp | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc48_open_timeline` | ignored | **PORT** | UC48: open timeline | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc57_move_buffer_to_another_window` | active | **PORT** | UC57: move buffer to another window | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc58_template_with_tab_stops` | ignored | **PORT** | UC58: template with tab stops | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc77_session_restore` | active | **PORT** | UC77: session restore | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc79_broken_link_detection` | ignored | **PORT** | UC79: broken link detection | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc82_git_merge_conflict_detection` | ignored | **REWRITE** | External edits are reconciled against the stored base and conflicts remain visible. | D4 changes the external-writer protocol to three-way merge; replace the legacy Git-conflict fixture. |
| `crates/bloom-core/tests/e2e.rs::uc86_auto_save_dirty_flag` | active | **PORT** | UC86: auto save dirty flag | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc88_which_key_during_vim_grammar` | ignored | **PORT** | UC88: which key during vim grammar | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc92_persistent_undo_across_restart` | ignored | **PORT** | UC92: persistent undo across restart | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc94_picker_scrolling_in_large_vault` | active | **PORT** | UC94: picker scrolling in large vault | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::uc95_block_id_assignment` | active | **REWRITE** | Open and save a page, assigning the chosen seven-character IDs to eligible blocks. | D7 changes ID length and assignment migration; preserve the open-save assignment journey. |
| `crates/bloom-core/tests/e2e.rs::history_scrubber_opens_and_has_nodes` | active | **PORT** | history scrubber opens and has nodes | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::history_scrubber_navigation_updates_preview` | active | **PORT** | history scrubber navigation updates preview | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/tests/e2e.rs::section_mirror_child_ids_from_parse_tree` | active | **DROP** | No product journey; retire mirror section parsing. | D2 removes mirrors; this asserts a parser representation only. |
| `crates/bloom-core/tests/e2e.rs::section_mirror_structural_diff_detects_insert` | active | **DROP** | No product journey; retire mirror structural-diff machinery. | D2 removes mirrors; this asserts an internal diff helper only. |
| `crates/bloom-core/tests/e2e.rs::section_mirror_structural_diff_detects_removal` | active | **DROP** | No product journey; retire mirror structural-diff machinery. | D2 removes mirrors; this asserts an internal diff helper only. |
| `crates/bloom-vim/src/state.rs::test_initial_mode_is_normal` | active | **PORT** | initial mode is normal | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_i_enters_insert_mode` | active | **PORT** | i enters insert mode | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_esc_returns_to_normal` | active | **PORT** | esc returns to normal | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_a_enters_insert_after_cursor` | active | **PORT** | a enters insert after cursor | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_v_enters_visual_mode` | active | **PORT** | v enters visual mode | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_colon_enters_command_mode` | active | **PORT** | colon enters command mode | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_w_motion_moves_to_next_word` | active | **PORT** | w motion moves to next word | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_b_motion_moves_to_prev_word` | active | **PORT** | b motion moves to prev word | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dollar_motion_to_end_of_line` | active | **PORT** | dollar motion to end of line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_0_motion_to_start_of_line` | active | **PORT** | 0 motion to start of line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_gg_motion_to_document_start` | active | **PORT** | gg motion to document start | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_g_motion_to_document_end` | active | **PORT** | g motion to document end | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dd_deletes_line` | active | **PORT** | dd deletes line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dd_last_empty_line` | active | **PORT** | dd last empty line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_x_deletes_char` | active | **PORT** | x deletes char | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_2x_deletes_two_chars` | active | **PORT** | 2x deletes two chars | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_s_changes_one_char_and_enters_insert` | active | **PORT** | s changes one char and enters insert | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dw_deletes_word` | active | **PORT** | dw deletes word | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_cw_changes_word_and_enters_insert` | active | **PORT** | cw changes word and enters insert | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_cw_single_letter_word_stays_on_current_word` | active | **PORT** | cw single letter word stays on current word | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_pending_keys_shown_after_d` | active | **PORT** | pending keys shown after d | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_yy_copies_to_unnamed_register` | active | **PORT** | yy copies to unnamed register | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_macro_recording_flag` | active | **PORT** | macro recording flag | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_count_prefix_with_motion` | active | **PORT** | count prefix with motion | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_h_moves_left` | active | **PORT** | h moves left | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_l_moves_right` | active | **PORT** | l moves right | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_j_moves_down` | active | **PORT** | j moves down | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_o_opens_line_below_and_enters_insert` | active | **PORT** | o opens line below and enters insert | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_o_upper_opens_line_above_and_enters_insert` | active | **PORT** | o upper opens line above and enters insert | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dd_last_content_line_cursor_goes_to_previous_line` | active | **PORT** | dd last content line cursor goes to previous line | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_o_at_end_of_buffer_with_trailing_newline` | active | **PORT** | o at end of buffer with trailing newline | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dd_then_o_single_line_jump` | active | **PORT** | dd then o single line jump | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_insert_mode_chars_are_edits` | active | **PORT** | insert mode chars are edits | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dot_repeat_records_dw` | active | **PORT** | dot repeat records dw | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dot_repeat_records_x` | active | **PORT** | dot repeat records x | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dot_repeat_records_insert_session` | active | **PORT** | dot repeat records insert session | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dot_repeat_records_cw_with_insert_text` | active | **PORT** | dot repeat records cw with insert text | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dot_repeat_records_o_with_insert_text` | active | **PORT** | dot repeat records o with insert text | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_dot_repeat_records_a_with_insert_text` | active | **PORT** | dot repeat records a with insert text | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_motion_does_not_record` | active | **PORT** | motion does not record | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_normal_edit_overwrites_insert_recording` | active | **PORT** | normal edit overwrites insert recording | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-vim/src/state.rs::test_replaying_suppresses_recording` | active | **PORT** | replaying suppresses recording | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-test-harness/tests/animations.rs::anim_basic_editing` | active | **PORT** | anim basic editing | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-test-harness/tests/animations.rs::anim_search` | active | **PORT** | anim search | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-test-harness/tests/animations.rs::anim_block_history` | active | **PORT** | anim block history | Preserve this behavior; no accepted decision changes the user outcome. |
| `crates/bloom-core/src/lib.rs::test_journal_scrubber_deadline_is_consumed_once` | active | **PORT** | An expired journal-scrubber deadline is consumed exactly once; subsequent ticks make progress. | F1 regression; preserve and promote to the I10 due-timer simulation detector. |
| `crates/bloom-core/src/lib.rs::test_block_id_assignment_waits_for_index_and_updates_known_ids` | active | **REWRITE** | Assign unique IDs across open pages without relying on indexer readiness. | F13 regression: replace index-readiness coupling with durable uniqueness (I4/L1). |
