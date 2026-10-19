use clap::ValueEnum;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, List, ListState, Paragraph, StatefulWidget, Widget},
};

use crate::app::Job;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum JobColumn {
    State,
    Id,
    Partition,
    User,
    Time,
    Name,
}

impl JobColumn {
    fn span(self, job: &Job) -> Span<'_> {
        match self {
            Self::State => Span::raw(&job.state_compact),
            Self::Id => Span::styled(job.id(), Style::default().fg(Color::Yellow)),
            Self::Partition => Span::styled(&job.partition, Style::default().fg(Color::Blue)),
            Self::User => Span::styled(&job.user, Style::default().fg(Color::Green)),
            Self::Time => Span::styled(&job.time, Style::default().fg(Color::Red)),
            Self::Name => Span::raw(&job.name),
        }
    }
}

pub fn job_lines<'a>(jobs: &'a [Job], columns: &[JobColumn]) -> Vec<Line<'a>> {
    let widths: Vec<usize> = columns
        .iter()
        .map(|column| {
            jobs.iter()
                .map(|job| column.span(job).width())
                .max()
                .unwrap_or(0)
        })
        .collect();
    jobs.iter()
        .map(|job| {
            let mut spans = Vec::new();
            for (index, column) in columns.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::raw(" "));
                }
                let mut span = column.span(job);
                let padding = widths[index] - span.width();
                if *column == JobColumn::Time {
                    span.content = format!("{}{}", " ".repeat(padding), span.content).into();
                } else if index + 1 < columns.len() {
                    span.content = format!("{}{}", span.content, " ".repeat(padding)).into();
                }
                spans.push(span);
            }
            Line::from(spans)
        })
        .collect()
}

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

    fn job(id: &str, user: &str, time: &str, name: &str) -> Job {
        Job {
            job_id: id.into(),
            array_id: String::new(),
            array_step: None,
            name: name.into(),
            state: "RUNNING".into(),
            state_compact: "R".into(),
            reason: None,
            user: user.into(),
            time: time.into(),
            time_limit: String::new(),
            start_time: String::new(),
            tres: String::new(),
            partition: "cpu".into(),
            nodelist: String::new(),
            stdout: None,
            stderr: None,
            command: String::new(),
        }
    }

    #[test]
    fn default_columns_preserve_alignment_and_colors() {
        let mut jobs = [
            job("7", "a", "1:00", "first"),
            job("123", "bob", "12:00", "longer"),
        ];
        jobs[1].state_compact = "PD".into();
        let lines = job_lines(
            &jobs,
            &[
                JobColumn::State,
                JobColumn::Id,
                JobColumn::Partition,
                JobColumn::User,
                JobColumn::Time,
                JobColumn::Name,
            ],
        );
        assert_eq!(lines[0].to_string(), "R  7   cpu a    1:00 first");
        assert_eq!(lines[1].to_string(), "PD 123 cpu bob 12:00 longer");
        for (index, color) in [
            (2, Color::Yellow),
            (4, Color::Blue),
            (6, Color::Green),
            (8, Color::Red),
        ] {
            assert_eq!(lines[0].spans[index].style.fg, Some(color));
        }
    }

    #[test]
    fn columns_can_be_reordered_and_omitted_with_unicode_alignment() {
        let jobs = [
            job("7", "a", "1:00", "界"),
            job("123", "bob", "12:00", "abc"),
        ];
        let lines = job_lines(&jobs, &[JobColumn::Name, JobColumn::Id]);
        assert_eq!(lines[0].to_string(), "界  7");
        assert_eq!(lines[1].to_string(), "abc 123");
        assert_eq!(lines[0].spans[0].width(), lines[1].spans[0].width());
        assert_eq!(job_lines(&jobs, &[JobColumn::Name])[0].to_string(), "界");
        assert!(job_lines(&[], &[JobColumn::Name]).is_empty());
    }

    #[test]
    fn id_column_uses_array_task_id() {
        let mut job = job("123", "a", "1:00", "array");
        job.array_id = "100".into();
        job.array_step = Some("7".into());
        assert_eq!(job_lines(&[job], &[JobColumn::Id])[0].to_string(), "100_7");
    }

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
