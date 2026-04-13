use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, DragTarget, FocusTarget};

// Theme colors
pub const BG: Color = Color::Rgb(0x0d, 0x11, 0x17);
pub const PANEL_BG: Color = Color::Rgb(0x16, 0x1b, 0x22);
pub const BORDER: Color = Color::Rgb(0x30, 0x36, 0x3d);
pub const FOCUS_BORDER: Color = Color::Rgb(0x58, 0xa6, 0xff);
pub const TEXT: Color = Color::Rgb(0xe6, 0xed, 0xf3);
pub const TEXT_DIM: Color = Color::Rgb(0x8b, 0x94, 0x9e);
pub const ACCENT_GREEN: Color = Color::Rgb(0x3f, 0xb9, 0x50);
#[allow(dead_code)]
pub const ACCENT_YELLOW: Color = Color::Rgb(0xd2, 0x99, 0x22);
pub const HEADER_BG: Color = Color::Rgb(0x21, 0x26, 0x2d);
pub const ACTIVE_BG: Color = Color::Rgb(0x1c, 0x23, 0x33);

const MIN_TERMINAL_WIDTH: u16 = 40;
const MIN_TERMINAL_HEIGHT: u16 = 10;
const MIN_PANE_AREA_WIDTH: u16 = 20;

/// Render the entire UI.
pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();

    if area.width < MIN_TERMINAL_WIDTH || area.height < MIN_TERMINAL_HEIGHT {
        let msg = Paragraph::new("Terminal too small")
            .style(Style::default().fg(TEXT_DIM).bg(BG))
            .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    let bg_block = Block::default().style(Style::default().bg(BG));
    frame.render_widget(bg_block, area);

    // Layout: tab bar (1) | main area | status bar (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tab bar
            Constraint::Min(1),   // main area
            Constraint::Length(1), // status bar
        ])
        .split(area);

    render_tab_bar(app, frame, chunks[0]);
    render_main_area(app, frame, chunks[1]);
    render_status_bar(frame, chunks[2]);
}

/// Render the tab bar.
fn render_tab_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let mut spans = Vec::new();
    let mut tab_rects = Vec::new();
    let mut x = area.x;

    spans.push(Span::styled(" ", Style::default().bg(HEADER_BG)));
    x += 1;

    for (i, ws) in app.workspaces.iter().enumerate() {
        let is_active = i == app.active_tab;
        let label = format!(" {} ", ws.name);
        let label_width = label.len() as u16;

        let style = if is_active {
            Style::default().fg(TEXT).bg(ACTIVE_BG).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_DIM).bg(HEADER_BG)
        };

        spans.push(Span::styled(label, style));

        // Store rect for mouse click
        tab_rects.push((i, Rect::new(x, area.y, label_width, 1)));
        x += label_width;

        // Separator
        spans.push(Span::styled("│", Style::default().fg(BORDER).bg(HEADER_BG)));
        x += 1;
    }

    // [+] button
    let plus_label = " + ";
    let plus_style = Style::default().fg(ACCENT_GREEN).bg(HEADER_BG);
    spans.push(Span::styled(plus_label, plus_style));
    let plus_rect = Rect::new(x, area.y, plus_label.len() as u16, 1);
    x += plus_label.len() as u16;

    // Fill remaining space
    let remaining = area.width.saturating_sub(x - area.x);
    if remaining > 0 {
        spans.push(Span::styled(
            " ".repeat(remaining as usize),
            Style::default().bg(HEADER_BG),
        ));
    }

    app.last_tab_rects = tab_rects;
    app.last_new_tab_rect = Some(plus_rect);

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Render the main area: [file tree] | panes | [preview]
fn render_main_area(app: &mut App, frame: &mut Frame, area: Rect) {
    let tree_width = app.file_tree_width;
    let preview_width = app.preview_width;

    let mut has_tree = app.ws().file_tree_visible;
    let mut has_preview = app.ws().preview.is_active();

    let needed = MIN_PANE_AREA_WIDTH
        + if has_tree { tree_width } else { 0 }
        + if has_preview { preview_width } else { 0 };
    if area.width < needed && has_preview {
        has_preview = false;
    }
    let needed = MIN_PANE_AREA_WIDTH + if has_tree { tree_width } else { 0 };
    if area.width < needed && has_tree {
        has_tree = false;
    }

    let swapped = app.layout_swapped;

    // Build layout constraints based on swap state
    // Normal:  [tree] [panes] [preview]
    // Swapped: [tree] [preview] [panes]
    let mut constraints = Vec::new();
    if has_tree {
        constraints.push(Constraint::Length(tree_width));
    }
    if swapped && has_preview {
        constraints.push(Constraint::Length(preview_width));
    }
    constraints.push(Constraint::Min(20));
    if !swapped && has_preview {
        constraints.push(Constraint::Length(preview_width));
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;

    if has_tree {
        app.ws_mut().last_file_tree_rect = Some(chunks[idx]);
        render_file_tree(app, frame, chunks[idx]);
        idx += 1;
    } else {
        app.ws_mut().last_file_tree_rect = None;
    }

    if swapped && has_preview {
        app.ws_mut().last_preview_rect = Some(chunks[idx]);
        render_preview(app, frame, chunks[idx]);
        idx += 1;
    }

    render_panes(app, frame, chunks[idx]);
    idx += 1;

    if !swapped && has_preview {
        app.ws_mut().last_preview_rect = Some(chunks[idx]);
        render_preview(app, frame, chunks[idx]);
    }

    if !has_preview {
        app.ws_mut().last_preview_rect = None;
    }
}

/// Render the file tree sidebar.
fn render_file_tree(app: &mut App, frame: &mut Frame, area: Rect) {
    let is_focused = app.ws().focus_target == FocusTarget::FileTree;
    let is_border_active = matches!(
        app.dragging.or(app.hover_border),
        Some(DragTarget::FileTreeBorder)
    );
    let border_color = if is_border_active {
        ACCENT_GREEN
    } else if is_focused {
        FOCUS_BORDER
    } else {
        BORDER
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(" FILES ", Style::default().fg(TEXT)))
        .style(Style::default().bg(PANEL_BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_height = inner.height as usize;
    app.ws_mut().file_tree.ensure_visible(visible_height);

    let entries = app.ws().file_tree.visible_entries();
    let scroll = app.ws().file_tree.scroll_offset;
    let selected = app.ws().file_tree.selected_index;

    for (i, entry) in entries.iter().skip(scroll).take(visible_height).enumerate() {
        let y = inner.y + i as u16;
        let entry_index = scroll + i;

        let indent = "  ".repeat(entry.depth);
        let icon = if entry.is_dir {
            if entry.is_expanded { "\u{25bc} " } else { "\u{25b6} " }
        } else {
            "  "
        };

        let name = &entry.name;
        let display = format!("{}{}{}", indent, icon, name);
        let max_width = inner.width as usize;
        let truncated = truncate_to_width(&display, max_width);

        let style = if entry_index == selected {
            Style::default().fg(TEXT).bg(ACTIVE_BG).add_modifier(Modifier::BOLD)
        } else if entry.is_dir {
            Style::default().fg(FOCUS_BORDER)
        } else {
            Style::default().fg(TEXT_DIM)
        };

        let line = Paragraph::new(truncated).style(style);
        frame.render_widget(line, Rect::new(inner.x, y, inner.width, 1));
    }
}

/// Render all panes.
fn render_panes(app: &mut App, frame: &mut Frame, area: Rect) {
    let rects = app.ws().layout.calculate_rects(area);
    app.ws_mut().last_pane_rects = rects.clone();

    for &(pane_id, rect) in &rects {
        if let Some(pane) = app.ws_mut().panes.get_mut(&pane_id) {
            let inner_rows = rect.height.saturating_sub(2);
            let inner_cols = rect.width.saturating_sub(2);
            let _ = pane.resize(inner_rows, inner_cols);
        }
    }

    let focused_id = app.ws().focused_pane_id;
    let focus_target = app.ws().focus_target;
    for (pane_id, rect) in rects {
        if let Some(pane) = app.ws().panes.get(&pane_id) {
            let is_focused = pane_id == focused_id && focus_target == FocusTarget::Pane;
            render_single_pane(pane, is_focused, frame, rect);
        }
    }
}

/// Render a single pane.
fn render_single_pane(pane: &crate::pane::Pane, is_focused: bool, frame: &mut Frame, area: Rect) {
    let border_color = if is_focused { FOCUS_BORDER } else { BORDER };

    let scroll_indicator = if pane.is_scrolled_back() { " [SCROLL]" } else { "" };
    let pane_title = if is_focused {
        format!(" claude [{}] \u{25cf}{} ", pane.id, scroll_indicator)
    } else {
        format!(" claude [{}]{} ", pane.id, scroll_indicator)
    };

    let title_style = if is_focused {
        Style::default().fg(ACCENT_GREEN)
    } else {
        Style::default().fg(TEXT_DIM)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(pane_title, title_style))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if pane.exited {
        let msg = Paragraph::new("[Process exited]")
            .style(Style::default().fg(TEXT_DIM).bg(BG))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
    } else {
        render_terminal_content(pane, is_focused, frame, inner);
    }
}

/// Render terminal content from vt100.
fn render_terminal_content(
    pane: &crate::pane::Pane,
    is_focused: bool,
    frame: &mut Frame,
    area: Rect,
) {
    let parser = pane.parser.lock().unwrap_or_else(|e| e.into_inner());
    let screen = parser.screen();

    let rows = area.height as usize;
    let cols = area.width as usize;
    let buf = frame.buffer_mut();

    for row in 0..rows {
        for col in 0..cols {
            let cell = screen.cell(row as u16, col as u16);
            if let Some(cell) = cell {
                let x = area.x + col as u16;
                let y = area.y + row as u16;

                let contents = cell.contents();
                let display_char = if contents.is_empty() { " " } else { contents };

                let fg = vt100_color_to_ratatui(cell.fgcolor());
                let bg = vt100_color_to_ratatui(cell.bgcolor());

                let mut modifiers = Modifier::empty();
                if cell.bold() { modifiers |= Modifier::BOLD; }
                if cell.italic() { modifiers |= Modifier::ITALIC; }
                if cell.underline() { modifiers |= Modifier::UNDERLINED; }

                let style = if cell.inverse() {
                    Style::default().fg(bg).bg(fg).add_modifier(modifiers)
                } else {
                    Style::default().fg(fg).bg(bg).add_modifier(modifiers)
                };

                if let Some(buf_cell) = buf.cell_mut((x, y)) {
                    buf_cell.set_symbol(display_char);
                    buf_cell.set_style(style);
                }
            }
        }
    }

    if is_focused && !screen.hide_cursor() {
        let cursor = screen.cursor_position();
        let cursor_x = area.x + cursor.1;
        let cursor_y = area.y + cursor.0;
        if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

/// Render file preview panel.
fn render_preview(app: &App, frame: &mut Frame, area: Rect) {
    let ws = app.ws();
    let filename = ws.preview.filename();
    let title = format!(" {} ", filename);

    let is_border_active = matches!(
        app.dragging.or(app.hover_border),
        Some(DragTarget::PreviewBorder)
    );
    let border_color = if is_border_active { ACCENT_GREEN } else { BORDER };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(title, Style::default().fg(TEXT)))
        .style(Style::default().bg(PANEL_BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if ws.preview.is_binary {
        let msg = Paragraph::new("バイナリファイルです")
            .style(Style::default().fg(TEXT_DIM).bg(PANEL_BG));
        frame.render_widget(msg, inner);
        return;
    }

    let visible_height = inner.height as usize;
    let scroll = ws.preview.scroll_offset;
    let has_highlights = !ws.preview.highlighted_lines.is_empty();

    for i in 0..visible_height {
        let line_idx = scroll + i;
        if line_idx >= ws.preview.lines.len() {
            break;
        }

        let y = inner.y + i as u16;
        let line_num = line_idx + 1;
        let num_str = format!("{:>4} ", line_num);
        let max_content = (inner.width as usize).saturating_sub(5);

        let mut spans = vec![Span::styled(num_str, Style::default().fg(TEXT_DIM))];

        if has_highlights && line_idx < ws.preview.highlighted_lines.len() {
            let mut used_width = 0;
            for styled_span in &ws.preview.highlighted_lines[line_idx] {
                if used_width >= max_content {
                    break;
                }
                let remaining = max_content - used_width;
                let text = truncate_to_width(&styled_span.text, remaining);
                used_width += unicode_width::UnicodeWidthStr::width(text.as_str());
                let (r, g, b) = styled_span.fg;
                spans.push(Span::styled(text, Style::default().fg(Color::Rgb(r, g, b))));
            }
        } else {
            let content = truncate_to_width(&ws.preview.lines[line_idx], max_content);
            spans.push(Span::styled(content, Style::default().fg(TEXT)));
        }

        let paragraph = Paragraph::new(Line::from(spans)).style(Style::default().bg(PANEL_BG));
        frame.render_widget(paragraph, Rect::new(inner.x, y, inner.width, 1));
    }
}

/// Truncate to display width (CJK aware).
fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut width = 0;
    for ch in s.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max_width {
            break;
        }
        result.push(ch);
        width += ch_width;
    }
    result
}

fn vt100_color_to_ratatui(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Render the status bar.
fn render_status_bar(frame: &mut Frame, area: Rect) {
    let hints = Line::from(vec![
        Span::styled(" ^D", Style::default().fg(FOCUS_BORDER)),
        Span::styled(" 縦分割  ", Style::default().fg(TEXT_DIM)),
        Span::styled("^E", Style::default().fg(FOCUS_BORDER)),
        Span::styled(" 横分割  ", Style::default().fg(TEXT_DIM)),
        Span::styled("^W", Style::default().fg(FOCUS_BORDER)),
        Span::styled(" 閉じる  ", Style::default().fg(TEXT_DIM)),
        Span::styled("^T", Style::default().fg(FOCUS_BORDER)),
        Span::styled(" 新タブ  ", Style::default().fg(TEXT_DIM)),
        Span::styled("^F", Style::default().fg(FOCUS_BORDER)),
        Span::styled(" ツリー  ", Style::default().fg(TEXT_DIM)),
        Span::styled("^P", Style::default().fg(FOCUS_BORDER)),
        Span::styled(" 配置替  ", Style::default().fg(TEXT_DIM)),
        Span::styled("^Q", Style::default().fg(FOCUS_BORDER)),
        Span::styled(" 終了", Style::default().fg(TEXT_DIM)),
    ]);

    let status = Paragraph::new(hints).style(Style::default().bg(HEADER_BG));
    frame.render_widget(status, area);
}
