use crossbeam::{
    channel::{Receiver, TryRecvError, unbounded},
    select,
};
use itertools::Either;
use std::{cmp::min, iter::once, path::PathBuf, process::Command, time::Duration};

use crate::file_watcher::{FileWatcherError, FileWatcherHandle};
use crate::job_watcher::JobWatcherHandle;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame, Terminal,
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io;
use tui_input::{Input, backend::crossterm::EventHandler};

pub enum Focus {
    Jobs,
}

pub enum Dialog {
    ConfirmCancelJob(String),
    SelectCancelSignal { id: String, selected_signal: usize },
    EditTimeLimit { id: String, input: Input },
    CommandError { command: String, output: String },
}

struct CommandFailure {
    command: String,
    output: String,
}

impl From<CommandFailure> for Dialog {
    fn from(failure: CommandFailure) -> Self {
        Dialog::CommandError {
            command: failure.command,
            output: failure.output,
        }
    }
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
    job_list_area: Rect,
    job_output_area: Rect,
    pending_input_event: Option<Event>,
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
    pub time_limit: String,
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
    MouseClick(usize),
    MouseWheel {
        target: MouseScrollTarget,
        direction: MouseWheelDirection,
        amount: u16,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseWheelDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseScrollTarget {
    Jobs,
    Output,
}

const SCANCEL_SIGNALS: &[&str] = &["TERM", "INT", "HUP", "USR1", "USR2", "STOP", "CONT", "KILL"];
const DIALOG_WIDTH: u16 = 80;
const FAST_SCROLL_LINES: u16 = 50;

fn page_scroll_delta(modifiers: KeyModifiers) -> u16 {
    if modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT) {
        FAST_SCROLL_LINES
    } else {
        1
    }
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
            job_list_area: Rect::default(),
            job_output_area: Rect::default(),
            pending_input_event: None,
        }
    }
}

impl App {
    pub fn run<B: Backend<Error = io::Error>>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> io::Result<()> {
        terminal.draw(|f| self.ui(f))?;

        loop {
            let (should_quit, should_draw) = if let Some(event) = self.pending_input_event.take() {
                self.handle_input_event(event)
            } else {
                select! {
                    recv(self.receiver) -> event => {
                        self.handle(event.unwrap());
                        (false, true)
                    }
                    recv(self.input_receiver) -> input_res => {
                        self.handle_input_event(input_res.unwrap().unwrap())
                    }
                }
            };
            if should_quit {
                return Ok(());
            }

            if should_draw {
                terminal.draw(|f| self.ui(f))?;
            }
        }
    }

    fn try_recv_input_event(&mut self) -> Option<Event> {
        if let Some(event) = self.pending_input_event.take() {
            return Some(event);
        }

        loop {
            match self.input_receiver.try_recv() {
                Ok(Ok(event)) => return Some(event),
                Ok(Err(_)) => continue,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return None,
            }
        }
    }

    fn handle_input_event(&mut self, event: Event) -> (bool, bool) {
        match event {
            Event::Key(key) => {
                if key.code == KeyCode::Char('q') {
                    return (true, false);
                }
                self.handle(AppMessage::Key(key));
                (false, true)
            }
            Event::Paste(_) => (false, false),
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if self.dialog.is_some() {
                        return (false, false);
                    }
                    if let Some(index) = self.job_index_at(mouse.column, mouse.row) {
                        if self.job_list_state.selected() != Some(index) {
                            self.handle(AppMessage::MouseClick(index));
                            return (false, true);
                        }
                    }
                    (false, false)
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    if self.dialog.is_some() {
                        return (false, false);
                    }
                    let Some(target) = self.mouse_scroll_target(mouse.column, mouse.row) else {
                        return (false, false);
                    };
                    let direction = mouse_wheel_direction(mouse.kind).unwrap();
                    let mut amount = 1u16;
                    while let Some(next_event) = self.try_recv_input_event() {
                        let should_merge = if let Event::Mouse(next_mouse) = &next_event {
                            mouse_wheel_direction(next_mouse.kind) == Some(direction)
                                && self.mouse_scroll_target(next_mouse.column, next_mouse.row)
                                    == Some(target)
                        } else {
                            false
                        };
                        if should_merge {
                            amount = amount.saturating_add(1);
                        } else {
                            self.pending_input_event = Some(next_event);
                            break;
                        }
                    }
                    self.handle(AppMessage::MouseWheel {
                        target,
                        direction,
                        amount,
                    });
                    (false, true)
                }
                _ => (false, false),
            },
            Event::Resize(_, _) => (false, true),
            _ => (false, false),
        }
    }

    fn mouse_scroll_target(&self, column: u16, row: u16) -> Option<MouseScrollTarget> {
        if rect_contains(self.job_list_area, column, row) {
            Some(MouseScrollTarget::Jobs)
        } else if rect_contains(self.job_output_area, column, row) {
            Some(MouseScrollTarget::Output)
        } else {
            None
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
                if self.dialog.is_some() {
                    let mut close_dialog = false;
                    let mut scancel_request = None;
                    let mut timelimit_request = None;
                    let mut command_failure = None;

                    match self.dialog.as_mut().expect("dialog must exist") {
                        Dialog::ConfirmCancelJob(id) => match key.code {
                            KeyCode::Enter | KeyCode::Char('y') => {
                                scancel_request = Some((id.clone(), None));
                                close_dialog = true;
                            }
                            KeyCode::Esc => {
                                close_dialog = true;
                            }
                            _ => {}
                        },
                        Dialog::SelectCancelSignal {
                            id,
                            selected_signal,
                        } => match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                *selected_signal = selected_signal.saturating_sub(1);
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                *selected_signal = min(
                                    selected_signal.saturating_add(1),
                                    SCANCEL_SIGNALS.len().saturating_sub(1),
                                );
                            }
                            KeyCode::Enter => {
                                scancel_request =
                                    Some((id.clone(), Some(SCANCEL_SIGNALS[*selected_signal])));
                                close_dialog = true;
                            }
                            KeyCode::Esc => {
                                close_dialog = true;
                            }
                            KeyCode::Char(c) if c.is_ascii_digit() => {
                                if let Some(index) = signal_index_for_digit(c) {
                                    if index < SCANCEL_SIGNALS.len() {
                                        *selected_signal = index;
                                    }
                                }
                            }
                            _ => {}
                        },
                        Dialog::EditTimeLimit { id, input } => match key.code {
                            KeyCode::Enter => {
                                if let Some(time_limit) = validated_time_limit(input) {
                                    timelimit_request = Some((id.clone(), time_limit));
                                    close_dialog = true;
                                }
                            }
                            KeyCode::Esc => {
                                close_dialog = true;
                            }
                            _ => {
                                input.handle_event(&Event::Key(key));
                            }
                        },
                        Dialog::CommandError { .. } => match key.code {
                            KeyCode::Enter | KeyCode::Esc => {
                                close_dialog = true;
                            }
                            _ => {}
                        },
                    };

                    if let Some((id, signal)) = scancel_request {
                        command_failure = execute_scancel(&id, signal).err();
                    }
                    if let Some((id, time_limit)) = timelimit_request {
                        command_failure = execute_scontrol_update_timelimit(&id, &time_limit).err();
                    }
                    if let Some(failure) = command_failure {
                        self.dialog = Some(failure.into());
                    } else if close_dialog {
                        self.dialog = None;
                    }
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
                            self.scroll_job_output_down_by(page_scroll_delta(key.modifiers));
                        }
                        KeyCode::PageUp => {
                            self.scroll_job_output_up_by(page_scroll_delta(key.modifiers));
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
                            if let Some(id) = self.selected_job_id() {
                                self.dialog = Some(Dialog::ConfirmCancelJob(id));
                            }
                        }
                        KeyCode::Char('C') => {
                            if let Some(id) = self.selected_job_id() {
                                self.dialog = Some(Dialog::SelectCancelSignal {
                                    id,
                                    selected_signal: 0,
                                });
                            }
                        }
                        KeyCode::Char('t') => {
                            if let Some(job) = self.selected_job() {
                                self.dialog = Some(Dialog::EditTimeLimit {
                                    id: job.id(),
                                    input: Input::new(job.time_limit.clone()),
                                });
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
            AppMessage::MouseClick(index) => {
                if self.dialog.is_none() && index < self.jobs.len() {
                    self.job_list_state.select(Some(index));
                }
            }
            AppMessage::MouseWheel {
                target,
                direction,
                amount,
            } => {
                if self.dialog.is_none() {
                    match target {
                        MouseScrollTarget::Jobs => match direction {
                            MouseWheelDirection::Up => self.job_list_state.scroll_up_by(amount),
                            MouseWheelDirection::Down => self.job_list_state.scroll_down_by(amount),
                        },
                        MouseScrollTarget::Output => match direction {
                            MouseWheelDirection::Up => self.scroll_job_output_up_by(amount),
                            MouseWheelDirection::Down => self.scroll_job_output_down_by(amount),
                        },
                    }
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
            ("c/C", "cancel/signal"),
            ("t", "set time limit"),
            ("o", "toggle stdout/stderr"),
            ("w", "toggle text wrap"),
        ];
        let help = Paragraph::new(help_line(&help_options));
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
        self.job_list_area = master_detail[0];

        // Job details

        let job_detail = self
            .job_list_state
            .selected()
            .and_then(|i| self.jobs.get(i));

        let job_detail = job_detail.map(|j| {
            let mut state = detail_line("State", &j.state);
            if j.state == "PENDING" {
                state.spans.extend([
                    Span::styled(" Start ", Style::default().fg(Color::Yellow)),
                    Span::raw(&j.start_time),
                ]);
            }
            if let Some(s) = j.reason.as_deref() {
                state.spans.extend([
                    Span::styled(" Reason ", Style::default().fg(Color::Yellow)),
                    Span::raw(s),
                ]);
            }
            let name = detail_line("Name", &j.name);
            let command = detail_line("Command", &j.command);
            let nodes = detail_line("Nodes", &j.nodelist);
            let tres = detail_line("TRES", &j.tres);
            let ui_stdout_text = match self.output_file_view {
                OutputFileView::Stdout => "stdout",
                OutputFileView::Stderr => "stderr",
            };
            let stdout = detail_line(
                ui_stdout_text,
                match self.output_file_view {
                    OutputFileView::Stdout => &j.stdout,
                    OutputFileView::Stderr => &j.stderr,
                }
                .as_ref()
                .map(|p| p.to_str().unwrap_or_default())
                .unwrap_or_default(),
            );

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
        self.job_output_area = log_area;
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
            match dialog {
                Dialog::ConfirmCancelJob(id) => {
                    let content = Text::from(Line::from(vec![
                        Span::raw("Cancel job "),
                        Span::styled(id, Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw("?"),
                    ]));

                    render_dialog(
                        f,
                        "Cancel",
                        Color::Green,
                        3,
                        content,
                        Some(Wrap { trim: true }),
                    );
                }
                Dialog::SelectCancelSignal {
                    id,
                    selected_signal,
                } => {
                    let mut rows = vec![
                        Line::from(vec![
                            Span::raw("Send signal to job "),
                            Span::styled(id, Style::default().add_modifier(Modifier::BOLD)),
                            Span::raw(":"),
                        ]),
                        Line::default(),
                    ];
                    rows.extend(SCANCEL_SIGNALS.iter().enumerate().map(|(i, signal)| {
                        let signal_style = if i == *selected_signal {
                            Style::default().fg(Color::Black).bg(Color::Green)
                        } else {
                            Style::default()
                        };
                        let shortcut_style = signal_style.add_modifier(Modifier::DIM);
                        Line::from(vec![
                            Span::styled(format!("{}. ", i + 1), shortcut_style),
                            Span::styled(*signal, signal_style),
                        ])
                    }));
                    let content = Text::from(rows);

                    render_dialog(
                        f,
                        "Signal",
                        Color::Green,
                        SCANCEL_SIGNALS.len() as u16 + 4,
                        content,
                        Some(Wrap { trim: true }),
                    );
                }
                Dialog::EditTimeLimit { id, input } => {
                    let area = centered_dialog_area(DIALOG_WIDTH, 3, f.area());
                    let inner = Block::default().borders(Borders::ALL).inner(area);

                    let prompt_prefix = "Set time limit for job ";
                    let prompt_suffix = ": ";
                    let prompt_width = (prompt_prefix.chars().count()
                        + id.chars().count()
                        + prompt_suffix.chars().count())
                        as u16;
                    let available_width = inner.width.saturating_sub(prompt_width).max(1) as usize;
                    let scroll = input.visual_scroll(available_width);
                    let visible_value = input
                        .value()
                        .chars()
                        .skip(scroll)
                        .take(available_width)
                        .collect::<String>();
                    let content = Text::from(Line::from(vec![
                        Span::raw(prompt_prefix),
                        Span::styled(id, Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(prompt_suffix),
                        Span::styled(visible_value, Style::default().fg(Color::Blue)),
                    ]));

                    let inner = render_dialog(f, "Time Limit", Color::Green, 3, content, None);

                    let cursor_offset = input.visual_cursor().saturating_sub(scroll) as u16;
                    let cursor_x = inner
                        .x
                        .saturating_add(prompt_width)
                        .saturating_add(cursor_offset)
                        .min(inner.x.saturating_add(inner.width.saturating_sub(1)));
                    let cursor_y = inner.y;
                    f.set_cursor_position((cursor_x, cursor_y));
                }
                Dialog::CommandError { command, output } => {
                    let dialog_text = format!("Command: {command}\n\n{output}");
                    let lines = dialog_text
                        .lines()
                        .count()
                        .saturating_add(2)
                        .min(u16::MAX as usize) as u16;
                    let content = Text::from(dialog_text);

                    render_dialog(
                        f,
                        "Command Error",
                        Color::Red,
                        lines,
                        content,
                        Some(Wrap { trim: false }),
                    );
                }
            }
        }
    }
}

fn centered_dialog_area(width: u16, lines: u16, viewport: Rect) -> Rect {
    let dialog_width = min(width, viewport.width);
    let dialog_height = min(lines, viewport.height);
    let dialog_x = viewport.x + viewport.width.saturating_sub(dialog_width) / 2;
    let dialog_y = viewport.y + viewport.height.saturating_sub(dialog_height) / 2;

    Rect::new(dialog_x, dialog_y, dialog_width, dialog_height)
}

/// Renders a dialog box scaffold (bordered block, centered and cleared) with the given
/// content, returning the block's inner `Rect` for callers that need to lay out further
/// details (e.g. a text cursor) relative to it.
fn render_dialog(
    f: &mut Frame,
    title: &str,
    color: Color,
    height: u16,
    content: Text,
    wrap: Option<Wrap>,
) -> Rect {
    let block = Block::default()
        .title(format!("─{title}"))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(color));

    let area = centered_dialog_area(DIALOG_WIDTH, height, f.area());
    let inner = block.inner(area);

    let mut paragraph = Paragraph::new(content)
        .style(Style::default().fg(Color::White))
        .block(block);
    if let Some(wrap) = wrap {
        paragraph = paragraph.wrap(wrap);
    }

    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);

    inner
}

fn help_line<'a>(entries: &[(&'a str, &'a str)]) -> Line<'a> {
    let blue_style = Style::default().fg(Color::Blue);
    let light_blue_style = Style::default().fg(Color::LightBlue);

    Line::from(
        entries
            .iter()
            .fold(Vec::new(), |mut acc, (key, description)| {
                if !acc.is_empty() {
                    acc.push(Span::raw(" | "));
                }
                acc.push(Span::styled(*key, blue_style));
                acc.push(Span::raw(": "));
                acc.push(Span::styled(*description, light_blue_style));
                acc
            }),
    )
}

fn detail_line<'a>(label: &str, value: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label:<7}"), Style::default().fg(Color::Yellow)),
        Span::raw(" "),
        Span::raw(value),
    ])
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
    fn selected_job(&self) -> Option<&Job> {
        self.job_list_state
            .selected()
            .and_then(|i| self.jobs.get(i))
    }

    fn selected_job_id(&self) -> Option<String> {
        self.selected_job().map(Job::id)
    }

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

    fn job_index_at(&self, column: u16, row: u16) -> Option<usize> {
        if self.jobs.is_empty() {
            return None;
        }
        let inner = Rect::new(
            self.job_list_area.x.saturating_add(1),
            self.job_list_area.y.saturating_add(1),
            self.job_list_area.width.saturating_sub(2),
            self.job_list_area.height.saturating_sub(2),
        );
        if !rect_contains(inner, column, row) {
            return None;
        }

        let row_in_list = (row - inner.y) as usize;
        let index = self.job_list_state.offset().saturating_add(row_in_list);
        (index < self.jobs.len()).then_some(index)
    }

    fn scroll_job_output_down_by(&mut self, delta: u16) {
        match self.job_output_anchor {
            ScrollAnchor::Top => {
                self.job_output_offset = self.job_output_offset.saturating_add(delta)
            }
            ScrollAnchor::Bottom => {
                self.job_output_offset = self.job_output_offset.saturating_sub(delta)
            }
        }
    }

    fn scroll_job_output_up_by(&mut self, delta: u16) {
        match self.job_output_anchor {
            ScrollAnchor::Top => {
                self.job_output_offset = self.job_output_offset.saturating_sub(delta)
            }
            ScrollAnchor::Bottom => {
                self.job_output_offset = self.job_output_offset.saturating_add(delta)
            }
        }
    }
}

fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

fn mouse_wheel_direction(kind: MouseEventKind) -> Option<MouseWheelDirection> {
    match kind {
        MouseEventKind::ScrollUp => Some(MouseWheelDirection::Up),
        MouseEventKind::ScrollDown => Some(MouseWheelDirection::Down),
        _ => None,
    }
}

fn signal_index_for_digit(digit: char) -> Option<usize> {
    let value = digit.to_digit(10)? as usize;
    if value == 0 { None } else { Some(value - 1) }
}

fn validated_time_limit(input: &Input) -> Option<String> {
    let time_limit = input.value().trim();
    if time_limit.is_empty() {
        None
    } else {
        Some(time_limit.to_string())
    }
}

fn execute_scancel(job_id: &str, signal: Option<&str>) -> Result<(), CommandFailure> {
    let mut command = Command::new("scancel");
    let mut command_display = String::from("scancel");

    if let Some(signal) = signal {
        command.arg("--signal").arg(signal);
        command_display.push_str(&format!(" --signal {signal}"));
    }
    command.arg(job_id);
    command_display.push_str(&format!(" {job_id}"));

    execute_command(command, command_display)
}

fn execute_scontrol_update_timelimit(job_id: &str, time_limit: &str) -> Result<(), CommandFailure> {
    let mut command = Command::new("scontrol");
    command
        .arg("update")
        .arg(format!("JobId={job_id}"))
        .arg(format!("TimeLimit={time_limit}"));

    execute_command(
        command,
        format!("scontrol update JobId={job_id} TimeLimit={time_limit}"),
    )
}

fn execute_command(mut command: Command, command_label: String) -> Result<(), CommandFailure> {
    let output = command.output().map_err(|error| CommandFailure {
        command: command_label.clone(),
        output: error.to_string(),
    })?;

    if output.status.success() {
        return Ok(());
    }

    let mut details = vec![match output.status.code() {
        Some(code) => format!("Exit code: {code}"),
        None => "Exit code: N/A".to_string(),
    }];

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = stdout.trim_end();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim_end();
    let has_stdout = !stdout.is_empty();
    let has_stderr = !stderr.is_empty();
    match (has_stdout, has_stderr) {
        (true, true) => {
            details.push(format!("stdout:\n{stdout}"));
            details.push(format!("stderr:\n{stderr}"));
        }
        (true, false) => {
            details.push(stdout.to_string());
        }
        (false, true) => {
            details.push(stderr.to_string());
        }
        (false, false) => {}
    }

    if details.len() == 1 {
        details.push("No output.".to_string());
    }

    Err(CommandFailure {
        command: command_label,
        output: details.join("\n\n"),
    })
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

    #[test]
    fn test_validated_time_limit() {
        assert_eq!(validated_time_limit(&Input::new("".to_string())), None);
        assert_eq!(validated_time_limit(&Input::new("   ".to_string())), None);
        assert_eq!(
            validated_time_limit(&Input::new(" 01:00:00 ".to_string())),
            Some("01:00:00".to_string())
        );
    }
}
