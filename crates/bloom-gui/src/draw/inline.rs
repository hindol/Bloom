use bloom_core::render::InlineMenuFrame;
use bloom_md::theme::ThemePalette;
use iced::Rectangle;

use crate::draw::{
    chars_that_fit, draw_text, draw_text_right, fill_panel, fill_rect, rect, truncate_text,
};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, LINE_HEIGHT};

/// Draw the inline menu within the pre-computed `area` rectangle.
pub(crate) fn draw_inline_menu(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    menu: &InlineMenuFrame,
    theme: &ThemePalette,
) {
    if menu.items.is_empty() {
        return;
    }

    let max_right = menu
        .items
        .iter()
        .filter_map(|item| item.right.as_ref())
        .map(|text| text.chars().count())
        .max()
        .unwrap_or(0);
    let visible_items = menu.items.len().min(8);

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
    let inner_x = area.x + CHAR_WIDTH / 2.0;
    let right_edge = area.x + area.width - CHAR_WIDTH / 2.0;
    let label_chars = chars_that_fit(area.width).saturating_sub(max_right + 5);

    for (visible_index, item) in menu
        .items
        .iter()
        .skip(scroll)
        .take(viewport)
        .enumerate()
    {
        let item_y = area.y + visible_index as f32 * LINE_HEIGHT + 2.0;
        let selected = scroll + visible_index == menu.selected;
        if selected {
            fill_rect(
                frame,
                rect(area.x + 1.0, item_y, area.width - 2.0, LINE_HEIGHT),
                rgb_to_color(&theme.mild),
            );
        }
        let label = truncate_text(&item.label, label_chars);
        draw_text(
            frame,
            inner_x,
            item_y,
            format!(" {}", label),
            rgb_to_color(&theme.foreground),
        );
        if let Some(right) = &item.right {
            let right = truncate_text(right, max_right);
            draw_text_right(
                frame,
                right_edge,
                item_y,
                &right,
                rgb_to_color(&theme.faded),
            );
        }
    }

    if let Some(hint) = &menu.hint {
        let hint_y = area.y + visible_items as f32 * LINE_HEIGHT + 2.0;
        fill_rect(
            frame,
            rect(area.x + 1.0, hint_y, area.width - 2.0, LINE_HEIGHT),
            rgb_to_color(&theme.subtle),
        );
        draw_text(
            frame,
            inner_x,
            hint_y,
            truncate_text(hint, chars_that_fit(area.width).saturating_sub(2)),
            rgb_to_color(&theme.faded),
        );
    }
}
