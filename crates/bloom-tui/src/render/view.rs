//! Live Views overlay rendering for TUI.

use bloom_core::render::ViewFrame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style as RStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::theme::TuiTheme;

/// Draw the view overlay as a full-screen modal.
pub fn draw_view(f: &mut Frame, view: &ViewFrame, theme: &TuiTheme) {
    let area = f.area();
    
    // Clear the background
    f.render_widget(Clear, area);
    
    // Create main layout with borders
    let main_block = Block::default()
        .title(if view.is_prompt {
            "Query Prompt"
        } else {
            &view.title
        })
        .borders(Borders::ALL)
        .border_style(RStyle::default().fg(theme.accent_green()));
    
    let inner_area = main_block.inner(area);
    f.render_widget(main_block, area);
    
    // Split into query area (if prompt), content area, and footer
    let layout = if view.is_prompt {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Query input
                Constraint::Min(1),    // Results
                Constraint::Length(2), // Footer
            ])
            .split(inner_area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // Results
                Constraint::Length(2), // Footer
            ])
            .split(inner_area)
    };
    
    let (results_area, footer_area) = if view.is_prompt {
        // Render query input box
        let query_block = Block::default()
            .title("Query")
            .borders(Borders::ALL)
            .border_style(RStyle::default().fg(theme.mild()));
        
        let query_inner = query_block.inner(layout[0]);
        f.render_widget(query_block, layout[0]);
        
        // Render query text with cursor
        let cursor_pos = view.query_cursor;
        let query_text = if cursor_pos < view.query.len() {
            vec![
                Span::raw(&view.query[..cursor_pos]),
                Span::styled("█", RStyle::default().bg(theme.accent_green())),
                Span::raw(&view.query[cursor_pos..]),
            ]
        } else {
            vec![
                Span::raw(&view.query),
                Span::styled("█", RStyle::default().bg(theme.accent_green())),
            ]
        };
        
        let query_para = Paragraph::new(Line::from(query_text))
            .wrap(Wrap { trim: false });
        f.render_widget(query_para, query_inner);
        
        (layout[1], layout[2])
    } else {
        (layout[0], layout[1])
    };
    
    // Show error or results
    if let Some(error) = &view.error {
        let error_para = Paragraph::new(format!("Error: {}", error))
            .style(RStyle::default().fg(theme.critical()))
            .wrap(Wrap { trim: false });
        f.render_widget(error_para, results_area);
    } else if view.rows.is_empty() {
        let empty_para = Paragraph::new("No results")
            .style(RStyle::default().fg(theme.faded()))
            .wrap(Wrap { trim: false });
        f.render_widget(empty_para, results_area);
    } else {
        // Render results list
        let items: Vec<ListItem> = view.rows.iter().enumerate().map(|(i, row)| {
            let content = if row.is_task {
                // Format task row with checkbox
                let checkbox = if row.task_done { "[x]" } else { "[ ]" };
                let text = row.cells.join(" | ");
                format!("{} {}", checkbox, text)
            } else {
                row.cells.join(" | ")
            };
            
            let style = if i == view.selected {
                RStyle::default().bg(theme.mild()).add_modifier(Modifier::BOLD)
            } else if row.is_task && row.task_done {
                RStyle::default().fg(theme.faded())
            } else {
                RStyle::default()
            };
            
            ListItem::new(content).style(style)
        }).collect();
        
        let results_list = List::new(items)
            .highlight_style(RStyle::default().bg(theme.mild()));
        
        f.render_widget(results_list, results_area);
    }
    
    // Render footer with status and help
    let footer_text = if view.total == 0 {
        "No results | q/Esc: close".to_string()
    } else {
        format!(
            "{}/{} | j/k: navigate | Enter: jump | x: toggle | q/Esc: close",
            view.selected + 1,
            view.total
        )
    };
    
    let footer_para = Paragraph::new(footer_text)
        .style(RStyle::default().fg(theme.faded()))
        .wrap(Wrap { trim: false });
    f.render_widget(footer_para, footer_area);
}