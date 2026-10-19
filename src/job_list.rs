use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, List, ListState, Paragraph, StatefulWidget, Widget},
};

pub struct JobList<'a> {
    pub lines: Vec<Line<'a>>,
    pub block: Block<'a>,
    pub horizontal_offset: &'a mut u16,
}

impl StatefulWidget for JobList<'_> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let inner = self.block.inner(area);
        *self.horizontal_offset = (*self.horizontal_offset).min(
            self.lines
                .iter()
                .map(Line::width)
                .max()
                .unwrap_or(0)
                .saturating_sub(usize::from(inner.width))
                .min(usize::from(u16::MAX)) as u16,
        );
        StatefulWidget::render(
            List::new(std::iter::repeat_n("", self.lines.len())).block(self.block),
            area,
            buf,
            state,
        );
        for (index, line) in self
            .lines
            .into_iter()
            .enumerate()
            .skip(state.offset())
            .take(usize::from(inner.height))
        {
            let row = Rect::new(
                inner.x,
                inner.y + (index - state.offset()) as u16,
                inner.width,
                1,
            );
            Paragraph::new(line)
                .scroll((0, *self.horizontal_offset))
                .render(row, buf);
            if state.selected() == Some(index) {
                buf.set_style(row, Style::default().bg(Color::Green).fg(Color::Black));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Span;

    #[test]
    fn horizontal_scroll_preserves_styles_selection_and_vertical_navigation() {
        let lines = vec![
            Line::from(vec![
                Span::styled("ABCD", Style::default().fg(Color::Yellow)),
                Span::styled("EFGHIJ", Style::default().fg(Color::Blue)),
            ]),
            Line::from("0123456789"),
            Line::from("abcdefghij"),
        ];
        let mut state = ListState::default().with_selected(Some(1));
        let mut offset = 2;
        let mut buffer = Buffer::empty(Rect::new(0, 0, 8, 4));
        JobList {
            lines: lines.clone(),
            block: Block::bordered(),
            horizontal_offset: &mut offset,
        }
        .render(buffer.area, &mut buffer, &mut state);
        let mut expected = Buffer::with_lines(["┌──────┐", "│CDEFGH│", "│234567│", "└──────┘"]);
        expected.set_style(Rect::new(1, 1, 2, 1), Style::default().fg(Color::Yellow));
        expected.set_style(Rect::new(3, 1, 4, 1), Style::default().fg(Color::Blue));
        expected.set_style(
            Rect::new(1, 2, 6, 1),
            Style::default().fg(Color::Black).bg(Color::Green),
        );
        assert_eq!(buffer, expected);

        state.select_next();
        let mut buffer = Buffer::empty(buffer.area);
        JobList {
            lines,
            block: Block::bordered(),
            horizontal_offset: &mut offset,
        }
        .render(buffer.area, &mut buffer, &mut state);
        let mut expected = Buffer::with_lines(["┌──────┐", "│234567│", "│cdefgh│", "└──────┘"]);
        expected.set_style(
            Rect::new(1, 2, 6, 1),
            Style::default().fg(Color::Black).bg(Color::Green),
        );
        assert_eq!(buffer, expected);
        assert_eq!(state.offset(), 1);
        assert_eq!(state.selected(), Some(2));
        assert_eq!(offset, 2);
    }

    #[test]
    fn horizontal_scroll_uses_display_columns_and_keeps_graphemes_intact() {
        for (offset, content) in [(0, "a界e\u{301}"), (1, "界e\u{301} "), (3, "e\u{301}👩‍💻z")]
        {
            let mut offset = offset;
            let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 3));
            JobList {
                lines: vec![Line::from("a界e\u{301}👩‍💻z")],
                block: Block::bordered(),
                horizontal_offset: &mut offset,
            }
            .render(buffer.area, &mut buffer, &mut ListState::default());
            assert_eq!(
                buffer,
                Buffer::with_lines([
                    "┌────┐".to_string(),
                    format!("│{content}│"),
                    "└────┘".to_string()
                ])
            );
        }
    }

    #[test]
    fn horizontal_offset_clamps_when_content_or_pane_width_changes() {
        let mut offset = u16::MAX;
        let mut state = ListState::default();
        for (text, width, expected) in [
            ("0123456789", 6, 6),
            ("0123456789", 10, 2),
            ("short", 10, 0),
            ("", 10, 0),
        ] {
            let mut buffer = Buffer::empty(Rect::new(0, 0, width, 3));
            JobList {
                lines: if text.is_empty() {
                    vec![]
                } else {
                    vec![Line::from(text)]
                },
                block: Block::bordered(),
                horizontal_offset: &mut offset,
            }
            .render(buffer.area, &mut buffer, &mut state);
            assert_eq!(offset, expected);
        }
    }
}
