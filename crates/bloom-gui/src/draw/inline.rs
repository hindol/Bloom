use bloom_core::render::{InlineMenuAnchor, InlineMenuFrame, PaneFrame};
use bloom_md::theme::ThemePalette;
use iced::Size;

use crate::draw::{
    chars_that_fit, draw_text, draw_text_right, fill_panel, fill_rect, rect, truncate_text,
};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, LINE_HEIGHT};

pub(crate) fn draw_inline_menu(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    active_pane: Option<&PaneFrame>,
    menu: &InlineMenuFrame,
    theme: &ThemePalette,
) {
    if menu.items.is_empty() {
        return;
    }

    let max_label = menu
        .items
        .iter()
        .map(|item| item.label.chars().count())
        .max()
        .unwrap_or(0);
    let max_right = menu
        .items
        .iter()
        .filter_map(|item| item.right.as_ref())
        .map(|text| text.chars().count())
        .max()
        .unwrap_or(0);
    let visible_items = menu.items.len().min(8);
    let hint_rows = usize::from(menu.hint.is_some());
    let menu_chars = (max_label + max_right + 6).clamp(16, 56);
    let menu_w = menu_chars as f32 * CHAR_WIDTH;
    let menu_h = (visible_items + hint_rows) as f32 * LINE_HEIGHT + LINE_HEIGHT * 0.5;

    let (anchor_x, anchor_y) = match menu.anchor {
        InlineMenuAnchor::CommandLine => {
            if let Some(pane) = active_pane {
                let x = pane.rect.x as f32 * CHAR_WIDTH;
                let y = (pane.rect.y + pane.rect.content_height) as f32 * LINE_HEIGHT - menu_h - 4.0;
                (x, y)
            } else {
                (0.0, size.height - menu_h - LINE_HEIGHT - 4.0)
            }
        }
        InlineMenuAnchor::Cursor { line, col } => {
            if let Some(pane) = active_pane {
                let x = pane.rect.x as f32 * CHAR_WIDTH + col as f32 * CHAR_WIDTH;
                let mut y = pane.rect.y as f32 * LINE_HEIGHT + (line + 1) as f32 * LINE_HEIGHT;
                if y + menu_h > size.height {
                    y = (pane.rect.y as f32 * LINE_HEIGHT + line as f32 * LINE_HEIGHT - menu_h)
                        .max(0.0);
                }
                (x, y)
            } else {
                (col as f32 * CHAR_WIDTH, line as f32 * LINE_HEIGHT)
            }
        }
    };

    let x = anchor_x.min((size.width - menu_w - 4.0).max(0.0)).max(0.0);
    let y = anchor_y.min((size.height - menu_h - 4.0).max(0.0)).max(0.0);
    let area = rect(x, y, menu_w, menu_h);
    fill_panel(
        frame,
        area,
        rgb_to_color(&theme.background),
        rgb_to_color(&theme.faded),
    );

    let viewport = visible_items;
    let scroll = if menu.selected >= viewport {
        menu.selected - viewport + 1
    } else {
        0
    };
    let inner_x = x + CHAR_WIDTH / 2.0;
    let right_edge = x + menu_w - CHAR_WIDTH / 2.0;
    let label_chars = chars_that_fit(menu_w).saturating_sub(max_right + 5);

    for (visible_index, item) in menu
        .items
        .iter()
        .skip(scroll)
        .take(viewport)
        .enumerate()
    {
        let item_y = y + visible_index as f32 * LINE_HEIGHT + 2.0;
        let selected = scroll + visible_index == menu.selected;
        if selected {
            fill_rect(
                frame,
                rect(x + 1.0, item_y, menu_w - 2.0, LINE_HEIGHT),
                rgb_to_color(&theme.mild),
            );
        }
        let label = truncate_text(&item.label, label_chars);
        draw_text(
            frame,
            inner_x,
            item_y,
            format!(" {}", label),
            rgb_to_color(if selected { &theme.strong } else { &theme.foreground }),
        );
        if let Some(right) = &item.right {
            let right = truncate_text(right, max_right);
            draw_text_right(
                frame,
                right_edge,
                item_y,
                &right,
                rgb_to_color(if selected { &theme.foreground } else { &theme.faded }),
            );
        }
    }

    if let Some(hint) = &menu.hint {
        let hint_y = y + visible_items as f32 * LINE_HEIGHT + 2.0;
        fill_rect(
            frame,
            rect(x + 1.0, hint_y, menu_w - 2.0, LINE_HEIGHT),
            rgb_to_color(&theme.subtle),
        );
        draw_text(
            frame,
            inner_x,
            hint_y,
            truncate_text(hint, chars_that_fit(menu_w).saturating_sub(2)),
            rgb_to_color(&theme.faded),
        );
    }
}
