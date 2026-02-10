use crossbeam::{
    channel::{Receiver, unbounded},
    select,
};
use itertools::Either;
use std::{cmp::min, iter::once, path::PathBuf, process::Command};
use std::{process::Stdio, time::Duration};

use crate::file_watcher::{FileWatcherError, FileWatcherHandle};
use crate::job_watcher::JobWatcherHandle;

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{
    Frame, Terminal,
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io;

pub enum Focus {
    Jobs,
}

pub enum Dialog {
    ConfirmCancelJob(String),
}

#[derive(Clone, Copy)]
pub enum ScrollAnchor {
    Top,
    Bottom,
}

#[derive(Default)]
pub enum OutputFileView {
    #[default]
    Stdout,
    Stderr,
}

pub struct App {
    focus: Focus,
    dialog: Option<Dialog>,
    jobs: Vec<Job>,
    job_list_state: ListState,
    job_output: Result<String, FileWatcherError>,
    job_output_anchor: ScrollAnchor,
    job_output_offset: u16,
    job_output_wrap: bool,
    _job_watcher: JobWatcherHandle,
    job_output_watcher: FileWatcherHandle,
    // sender: Sender<AppMessage>,
    receiver: Receiver<AppMessage>,
    input_receiver: Receiver<std::io::Result<Event>>,
    output_file_view: OutputFileView,
    job_list_height: u16,
}

pub struct Job {
    pub job_id: String,
    pub array_id: String,
    pub array_step: Option<String>,
    pub name: String,
    pub state: String,
    pub state_compact: String,
    pub reason: Option<String>,
    pub user: String,
    pub time: String,
    pub start_time: String,
    pub tres: String,
    pub partition: String,
    pub nodelist: String,
    pub stdout: Option<PathBuf>,
    pub stderr: Option<PathBuf>,
    pub command: String,
}

impl Job {
    fn id(&self) -> String {
        match self.array_step.as_ref() {
            Some(array_step) => format!("{}_{}", self.array_id, array_step),
            None => self.job_id.clone(),
        }
    }
}

pub enum AppMessage {
    Jobs(Vec<Job>),
    JobOutput(Result<String, FileWatcherError>),
    Key(KeyEvent),
}

impl App {
    pub fn new(
        input_receiver: Receiver<std::io::Result<Event>>,
        slurm_refresh_rate: u64,
        file_refresh_rate: u64,
        squeue_args: Vec<String>,
    ) -> App {
        let (sender, receiver) = unbounded();
        Self {
            focus: Focus::Jobs,
            dialog: None,
            jobs: Vec::new(),
            _job_watcher: JobWatcherHandle::new(
                sender.clone(),
                Duration::from_secs(slurm_refresh_rate),
                squeue_args,
            ),
            job_list_state: ListState::default(),
            job_output: Ok("".to_string()),
            job_output_anchor: ScrollAnchor::Bottom,
            job_output_offset: 0,
            job_output_wrap: false,
            job_output_watcher: FileWatcherHandle::new(
                sender.clone(),
                Duration::from_secs(file_refresh_rate),
            ),
            // sender,
            receiver,
            input_receiver,
            output_file_view: OutputFileView::default(),
            job_list_height: 0,
        }
    }
}

impl App {
    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> io::Result<()> {
        terminal.draw(|f| self.ui(f))?;

        loop {
            select! {
                recv(self.receiver) -> event => {
                    self.handle(event.unwrap());
                }
                recv(self.input_receiver) -> input_res => {
                    match input_res.unwrap().unwrap() {
                        Event::Key(key) => {
                            if key.code == KeyCode::Char('q') {
                                return Ok(());
                            }
                            self.handle(AppMessage::Key(key));
                        },
                        Event::Resize(_, _) => {},
                        _ => continue, // ignore and do not redraw
                    }
                }
            };

            terminal.draw(|f| self.ui(f))?;
        }
    }

    fn handle(&mut self, msg: AppMessage) {
        match msg {
            AppMessage::Jobs(jobs) => {
                // On refresh: keep the same job selected if it still exists
                let old_index = self.job_list_state.selected();
                let old_id = old_index.and_then(|i| self.jobs.get(i)).map(|j| j.id());

                self.jobs = jobs;

                if self.jobs.is_empty() {
                    self.job_list_state.select(None);
                } else if let Some(id) = old_id {
                    let new_index = self
                        .jobs
                        .iter()
                        .position(|j| j.id() == id)
                        .unwrap_or(old_index.unwrap_or(0).min(self.jobs.len() - 1));
                    self.job_list_state.select(Some(new_index));
                } else {
                    self.job_list_state.select_first();
                }
            }
            AppMessage::JobOutput(content) => self.job_output = content,
            AppMessage::Key(key) => {
                if let Some(dialog) = &self.dialog {
                    match dialog {
                        Dialog::ConfirmCancelJob(id) => match key.code {
                            KeyCode::Enter | KeyCode::Char('y') => {
                                let id = id.clone();
                                Command::new("scancel")
                                    .arg(id)
                                    .stdout(Stdio::null())
                                    .stderr(Stdio::null())
                                    .status()
                                    .expect("failed to execute scancel");
                                self.dialog = None;
                            }
                            KeyCode::Esc => {
                                self.dialog = None;
                            }
                            _ => {}
                        },
                    };
                } else {
                    match key.code {
                        KeyCode::Char('h') | KeyCode::Left => self.focus_previous_panel(),
                        KeyCode::Char('l') | KeyCode::Right => self.focus_next_panel(),
                        KeyCode::Char('k') | KeyCode::Up => match self.focus {
                            Focus::Jobs => self.select_previous_job(),
                        },
                        KeyCode::Char('j') | KeyCode::Down => match self.focus {
                            Focus::Jobs => self.select_next_job(),
                        },
                        KeyCode::Char('g') => match self.focus {
                            Focus::Jobs => self.select_first_job(),
                        },
                        KeyCode::Char('G') => match self.focus {
                            Focus::Jobs => self.select_last_job(),
                        },
                        KeyCode::Char('u') => match self.focus {
                            Focus::Jobs => {
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL)
                                {
                                    self.scroll_jobs_half_page_up()
                                }
                            }
                        },
                        KeyCode::Char('d') => match self.focus {
                            Focus::Jobs => {
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::CONTROL)
                                {
                                    self.scroll_jobs_half_page_down()
                                }
                            }
                        },
                        KeyCode::PageDown => {
                            let delta = if key.modifiers.intersects(
                                crossterm::event::KeyModifiers::SHIFT
                                    | crossterm::event::KeyModifiers::CONTROL
                                    | crossterm::event::KeyModifiers::ALT,
                            ) {
                                50
                            } else {
                                1
                            };
                            match self.job_output_anchor {
                                ScrollAnchor::Top => {
                                    self.job_output_offset =
                                        self.job_output_offset.saturating_add(delta)
                                }
                                ScrollAnchor::Bottom => {
                                    self.job_output_offset =
                                        self.job_output_offset.saturating_sub(delta)
                                }
                            }
                        }
                        KeyCode::PageUp => {
                            let delta = if key.modifiers.intersects(
                                crossterm::event::KeyModifiers::SHIFT
                                    | crossterm::event::KeyModifiers::CONTROL
                                    | crossterm::event::KeyModifiers::ALT,
                            ) {
                                50
                            } else {
                                1
                            };
                            match self.job_output_anchor {
                                ScrollAnchor::Top => {
                                    self.job_output_offset =
                                        self.job_output_offset.saturating_sub(delta)
                                }
                                ScrollAnchor::Bottom => {
                                    self.job_output_offset =
                                        self.job_output_offset.saturating_add(delta)
                                }
                            }
                        }
                        KeyCode::Home => {
                            self.job_output_offset = 0;
                            self.job_output_anchor = ScrollAnchor::Top;
                        }
                        KeyCode::End => {
                            self.job_output_offset = 0;
                            self.job_output_anchor = ScrollAnchor::Bottom;
                        }
                        KeyCode::Char('c') => {
                            if let Some(id) = self
                                .job_list_state
                                .selected()
                                .and_then(|i| self.jobs.get(i).map(|j| j.id()))
                            {
                                self.dialog = Some(Dialog::ConfirmCancelJob(id));
                            }
                        }
                        KeyCode::Char('o') => {
                            self.output_file_view = match self.output_file_view {
                                OutputFileView::Stdout => OutputFileView::Stderr,
                                OutputFileView::Stderr => OutputFileView::Stdout,
                            };
                        }
                        KeyCode::Char('w') => {
                            self.job_output_wrap = !self.job_output_wrap;
                        }
                        _ => {}
                    };
                }
            }
        }

        // update
        self.job_output_watcher
            .set_file_path(self.job_list_state.selected().and_then(|i| {
                self.jobs.get(i).and_then(|j| match self.output_file_view {
                    OutputFileView::Stdout => j.stdout.clone(),
                    OutputFileView::Stderr => j.stderr.clone(),
                })
            }));
    }

    fn ui(&mut self, f: &mut Frame) {
        // Layout

        let content_help = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)].as_ref())
            .split(f.area());

        let master_detail = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(50), Constraint::Percentage(70)].as_ref())
            .split(content_help[0]);

        let job_detail_log = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Min(3)].as_ref())
            .split(master_detail[1]);

        // Help
        let help_options = vec![
            ("q", "quit"),
            ("⏶/⏷", "navigate"),
            ("pgup/pgdown", "scroll"),
            ("home/end", "top/bottom"),
            ("esc", "cancel"),
            ("enter", "confirm"),
            ("c", "cancel job"),
            ("o", "toggle stdout/stderr"),
            ("w", "toggle text wrap"),
        ];
        let blue_style = Style::default().fg(Color::Blue);
        let light_blue_style = Style::default().fg(Color::LightBlue);

        let help = Line::from(help_options.iter().fold(
            Vec::new(),
            |mut acc, (key, description)| {
                if !acc.is_empty() {
                    acc.push(Span::raw(" | "));
                }
                acc.push(Span::styled(*key, blue_style));
                acc.push(Span::raw(": "));
                acc.push(Span::styled(*description, light_blue_style));
                acc
            },
        ));

        let help = Paragraph::new(help);
        f.render_widget(help, content_help[1]);

        // Jobs
        let max_id_len = self.jobs.iter().map(|j| j.id().len()).max().unwrap_or(0);
        let max_user_len = self.jobs.iter().map(|j| j.user.len()).max().unwrap_or(0);
        let max_partition_len = self
            .jobs
            .iter()
            .map(|j| j.partition.len())
            .max()
            .unwrap_or(0);
        let max_time_len = self.jobs.iter().map(|j| j.time.len()).max().unwrap_or(0);
        let max_state_compact_len = self
            .jobs
            .iter()
            .map(|j| j.state_compact.len())
            .max()
            .unwrap_or(0);
        let jobs: Vec<ListItem> = self
            .jobs
            .iter()
            .map(|j| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(
                            "{:<max$.max$}",
                            j.state_compact,
                            max = max_state_compact_len
                        ),
                        Style::default(),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:<max$.max$}", j.id(), max = max_id_len),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:<max$.max$}", j.partition, max = max_partition_len),
                        Style::default().fg(Color::Blue),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:<max$.max$}", j.user, max = max_user_len),
                        Style::default().fg(Color::Green),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:>max$.max$}", j.time, max = max_time_len),
                        Style::default().fg(Color::Red),
                    ),
                    Span::raw(" "),
                    Span::raw(&j.name),
                ]))
            })
            .collect();
        let job_list = List::new(jobs)
            .block(
                Block::default()
                    .title(format!("─Jobs ({})", self.jobs.len()))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(if self.dialog.is_some() {
                        Style::default()
                    } else {
                        match self.focus {
                            Focus::Jobs => Style::default().fg(Color::Green),
                        }
                    }),
            )
            .highlight_style(Style::default().bg(Color::Green).fg(Color::Black));
        f.render_stateful_widget(job_list, master_detail[0], &mut self.job_list_state);
        self.job_list_height = master_detail[0].height.saturating_sub(2); // account for borders

        // Job details

        let job_detail = self
            .job_list_state
            .selected()
            .and_then(|i| self.jobs.get(i));

        let job_detail = job_detail.map(|j| {
            let mut state_spans = vec![
                Span::styled("State  ", Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::raw(&j.state),
            ];
            if j.state == "PENDING" {
                state_spans.extend([
                    Span::styled(" Start ", Style::default().fg(Color::Yellow)),
                    Span::raw(&j.start_time),
                ]);
            }
            if let Some(s) = j.reason.as_deref() {
                state_spans.extend([
                    Span::styled(" Reason ", Style::default().fg(Color::Yellow)),
                    Span::raw(s),
                ]);
            }
            let state = Line::from(state_spans);
            let name = Line::from(vec![
                Span::styled("Name   ", Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::raw(&j.name),
            ]);
            let command = Line::from(vec![
                Span::styled("Command", Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::raw(&j.command),
            ]);
            let nodes = Line::from(vec![
                Span::styled("Nodes  ", Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::raw(&j.nodelist),
            ]);
            let tres = Line::from(vec![
                Span::styled("TRES   ", Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::raw(&j.tres),
            ]);
            let ui_stdout_text = match self.output_file_view {
                OutputFileView::Stdout => "stdout ",
                OutputFileView::Stderr => "stderr ",
            };
            let stdout = Line::from(vec![
                Span::styled(ui_stdout_text, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::raw(
                    match self.output_file_view {
                        OutputFileView::Stdout => &j.stdout,
                        OutputFileView::Stderr => &j.stderr,
                    }
                    .as_ref()
                    .map(|p| p.to_str().unwrap_or_default())
                    .unwrap_or_default(),
                ),
            ]);

            Text::from(vec![state, name, command, nodes, tres, stdout])
        });
        let job_detail = Paragraph::new(job_detail.unwrap_or_default()).block(
            Block::default()
                .title("─Details")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
        f.render_widget(job_detail, job_detail_log[0]);

        // Log
        let log_area = job_detail_log[1];
        let log_title = Line::from(vec![
            Span::raw("─"),
            Span::raw(match self.output_file_view {
                OutputFileView::Stdout => "stdout",
                OutputFileView::Stderr => "stderr",
            }),
            Span::styled(
                match self.job_output_anchor {
                    ScrollAnchor::Top if self.job_output_offset == 0 => "[T]".to_string(),
                    ScrollAnchor::Top => format!("[T+{}]", self.job_output_offset),
                    ScrollAnchor::Bottom if self.job_output_offset == 0 => "".to_string(),
                    ScrollAnchor::Bottom => format!("[B-{}]", self.job_output_offset),
                },
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]);
        let log_block = Block::default().title(log_title).borders(Borders::ALL);
        let log_block = log_block.border_type(BorderType::Rounded);

        // let job_log = self.job_stdout.as_deref().map(|s| {
        //     string_for_paragraph(
        //         s,
        //         log_block.inner(log_area).height as usize,
        //         log_block.inner(log_area).width as usize,
        //         self.job_stdout_offset as usize,
        //     )
        // }).unwrap_or_else(|e| {
        //     self.job_stdout_offset = 0;
        //     "".to_string()
        // });

        let log = match self.job_output.as_deref() {
            Ok(s) => Paragraph::new(fit_text(
                s,
                log_block.inner(log_area).height as usize,
                log_block.inner(log_area).width as usize,
                self.job_output_anchor,
                self.job_output_offset as usize,
                self.job_output_wrap,
            )),
            Err(e) => Paragraph::new(e.to_string())
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: true }),
        }
        .block(log_block);

        f.render_widget(log, log_area);

        if let Some(dialog) = &self.dialog {
            fn centered_lines(percent_x: u16, lines: u16, r: Rect) -> Rect {
                let dy = r.height.saturating_sub(lines) / 2;
                let r = Rect::new(r.x, r.y + dy, r.width, min(lines, r.height - dy));

                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(
                        [
                            Constraint::Percentage((100 - percent_x) / 2),
                            Constraint::Percentage(percent_x),
                            Constraint::Percentage((100 - percent_x) / 2),
                        ]
                        .as_ref(),
                    )
                    .split(r)[1]
            }

            match dialog {
                Dialog::ConfirmCancelJob(id) => {
                    let dialog = Paragraph::new(Line::from(vec![
                        Span::raw("Cancel job "),
                        Span::styled(id, Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw("?"),
                    ]))
                    .style(Style::default().fg(Color::White))
                    .wrap(Wrap { trim: true })
                    .block(
                        Block::default()
                            .title("─Confirm")
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .style(Style::default().fg(Color::Green)),
                    );

                    let area = centered_lines(75, 3, f.area());
                    f.render_widget(Clear, area);
                    f.render_widget(dialog, area);
                }
            }
        }
    }
}

fn chunked_string(s: &str, first_chunk_size: usize, chunk_size: usize) -> Vec<&str> {
    let stepped_indices = s
        .char_indices()
        .map(|(i, _)| i)
        .enumerate()
        .filter(|&(i, _)| {
            if i > (first_chunk_size) {
                chunk_size > 0 && (i - first_chunk_size).is_multiple_of(chunk_size)
            } else {
                i == 0 || i == first_chunk_size
            }
        })
        .map(|(_, e)| e)
        .collect::<Vec<_>>();
    let windows = stepped_indices.windows(2).collect::<Vec<_>>();

    let iter = windows.iter().map(|w| &s[w[0]..w[1]]);
    let last_index = *stepped_indices.last().unwrap_or(&0);
    iter.chain(once(&s[last_index..])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunked_string() {
        // Divisible
        let input = "abcdefghij";
        let expected = vec!["abcd", "ef", "gh", "ij"];
        assert_eq!(chunked_string(input, 4, 2), expected);

        // Not divisible
        let input = "123456789";
        let expected = vec!["1234", "56", "78", "9"];
        assert_eq!(chunked_string(input, 4, 2), expected);

        // Smaller
        let input = "abc";
        let expected = vec!["abc"];
        assert_eq!(chunked_string(input, 4, 2), expected);

        // Smaller
        let input = "abcde";
        let expected = vec!["abcd", "e"];
        assert_eq!(chunked_string(input, 4, 2), expected);

        // Empty
        let input = "";
        let expected: Vec<&str> = vec![""];
        assert_eq!(chunked_string(input, 4, 2), expected);

        let input = "123456789";
        let expected = vec!["1234", "56789"];
        assert_eq!(chunked_string(input, 4, 0), expected);

        let input = "123456789";
        let expected = vec!["12", "34", "56", "78", "9"];
        assert_eq!(chunked_string(input, 0, 2), expected);

        let input = "123456789";
        let expected = vec!["123456789"];
        assert_eq!(chunked_string(input, 0, 0), expected);
    }
}

fn fit_text(
    s: &'_ str,
    lines: usize,
    cols: usize,
    anchor: ScrollAnchor,
    offset: usize,
    wrap: bool,
) -> Text<'_> {
    let s = s.rsplit_once(['\r', '\n']).map_or(s, |(p, _)| p); // skip everything after last line delimiter
    let l = s.lines().flat_map(|l| l.split('\r')); // bandaid for term escape codes
    let iter = match anchor {
        ScrollAnchor::Top => Either::Left(l),
        ScrollAnchor::Bottom => Either::Right(l.rev()),
    };
    let iter = iter
        .skip(offset)
        .flat_map(|l| {
            let iter = if wrap {
                Either::Left(
                    chunked_string(l, cols, cols.saturating_sub(2))
                        .into_iter()
                        .enumerate()
                        .map(|(i, l)| {
                            if i == 0 {
                                Line::raw(l.chars().take(cols).collect::<String>())
                            } else {
                                Line::default().spans(vec![
                                    Span::styled(
                                        "↪ ",
                                        Style::default().add_modifier(Modifier::DIM),
                                    ),
                                    Span::raw(
                                        l.chars().take(cols.saturating_sub(2)).collect::<String>(),
                                    ),
                                ])
                            }
                        }),
                )
            } else {
                match l.chars().nth(cols) {
                    Some(_) => {
                        // has more chars than cols
                        Either::Right(once(Line::default().spans(vec![
                            Span::raw(l.chars().take(cols.saturating_sub(1)).collect::<String>()),
                            Span::styled("…", Style::default().add_modifier(Modifier::DIM)),
                        ])))
                    }
                    None => {
                        Either::Right(once(Line::raw(l.chars().take(cols).collect::<String>())))
                    }
                }
            };
            match anchor {
                ScrollAnchor::Top => Either::Left(iter),
                ScrollAnchor::Bottom => Either::Right(iter.rev()),
            }
        })
        .take(lines);

    match anchor {
        ScrollAnchor::Top => Text::from(iter.collect::<Vec<_>>()),
        ScrollAnchor::Bottom => Text::from(
            iter.collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>(),
        ),
    }
}

impl App {
    fn focus_next_panel(&mut self) {
        match self.focus {
            Focus::Jobs => self.focus = Focus::Jobs,
        }
    }

    fn focus_previous_panel(&mut self) {
        match self.focus {
            Focus::Jobs => self.focus = Focus::Jobs,
        }
    }

    fn select_next_job(&mut self) {
        self.job_list_state.select_next();
    }

    fn select_previous_job(&mut self) {
        self.job_list_state.select_previous();
    }

    fn select_first_job(&mut self) {
        self.job_list_state.select_first();
    }

    fn select_last_job(&mut self) {
        self.job_list_state.select_last();
    }

    fn scroll_jobs_half_page_down(&mut self) {
        self.job_list_state.scroll_down_by(self.job_list_height / 2);
    }

    fn scroll_jobs_half_page_up(&mut self) {
        self.job_list_state.scroll_up_by(self.job_list_height / 2);
    }
}
