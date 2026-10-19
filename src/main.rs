mod app;
mod file_watcher;
mod input;
mod job_watcher;
mod pager;
mod squeue_args;

use app::{App, AppExit, Dialog};
use clap::CommandFactory;
use clap::Parser;
use clap::Subcommand;
use clap_complete::{Shell, generate};
use crossterm::{
    cursor::Show,
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use pager::Pager;
use ratatui::{Terminal, backend::CrosstermBackend};
use signal_hook::consts::{SIGINT, SIGQUIT};
use squeue_args::SqueueArgs;
use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{env, io, panic};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Refresh rate for the job watcher.
    #[arg(
        long,
        env = "TURM_SLURM_REFRESH",
        value_name = "SECONDS",
        default_value_t = 2
    )]
    slurm_refresh: u64,

    /// Refresh rate for the file watcher.
    #[arg(
        long,
        env = "TURM_FILE_REFRESH",
        value_name = "SECONDS",
        default_value_t = 2
    )]
    file_refresh: u64,

    /// Initial Jobs pane width as a percentage (0-100).
    #[arg(
        long,
        env = "TURM_JOBS_WIDTH",
        value_name = "PERCENT",
        default_value_t = 30,
        value_parser = clap::value_parser!(u16).range(0..=100)
    )]
    jobs_width: u16,

    /// Initial Details pane content lines, excluding borders (0 hides it).
    #[arg(
        long,
        env = "TURM_DETAILS_LINES",
        value_name = "LINES",
        default_value_t = app::MAX_DETAILS_LINES,
        value_parser = clap::value_parser!(u16).range(0..=i64::from(app::MAX_DETAILS_LINES))
    )]
    details_lines: u16,

    /// Log viewer command (falls back to PAGER, then less or more).
    #[arg(long, env = "TURM_PAGER", value_name = "COMMAND")]
    pager: Option<String>,

    /// squeue arguments
    #[command(flatten)]
    squeue_args: SqueueArgs,

    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Print shell completion script to stdout.
    Completion {
        /// The shell to generate completion for.
        shell: Shell,
    },
}

fn main() -> io::Result<()> {
    let args = Cli::parse();
    match args.command {
        Some(CliCommand::Completion { shell }) => {
            let cmd = &mut Cli::command();
            generate(shell, cmd, cmd.get_name().to_string(), &mut io::stdout());
            return Ok(());
        }
        None => {}
    }

    install_panic_hook();

    let keyboard_signals_default = Arc::new(AtomicBool::new(true));
    for signal in [SIGINT, SIGQUIT] {
        signal_hook::flag::register_conditional_default(
            signal,
            Arc::clone(&keyboard_signals_default),
        )?;
    }
    let mut terminal_guard = TerminalGuard::new(io::stdout())?;
    run_app(&mut terminal_guard, args, &keyboard_signals_default)
}

fn install_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableBracketedPaste,
            DisableMouseCapture,
            Show
        );
        default_hook(panic_info);
    }));
}

struct TerminalGuard<W: Write> {
    terminal: Terminal<CrosstermBackend<W>>,
}

impl<W: Write> TerminalGuard<W> {
    fn new(mut writer: W) -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(
            writer,
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture
        )?;
        let backend = CrosstermBackend::new(writer);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<W>> {
        &mut self.terminal
    }

    fn suspend(&mut self) -> io::Result<()> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableBracketedPaste,
            DisableMouseCapture
        )?;
        self.terminal.show_cursor()
    }

    fn resume(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture
        )?;
        self.terminal.clear()
    }
}

impl<W: Write> Drop for TerminalGuard<W> {
    fn drop(&mut self) {
        let _ = self.suspend();
    }
}

fn run_app<W: Write>(
    terminal: &mut TerminalGuard<W>,
    args: Cli,
    keyboard_signals_default: &AtomicBool,
) -> io::Result<()> {
    let pager = Pager::new(args.pager, env::var("PAGER").ok());
    let mut app = App::new(
        args.slurm_refresh,
        args.file_refresh,
        args.squeue_args.to_vec(),
        args.jobs_width,
        args.details_lines,
    );
    loop {
        match app.run(terminal.terminal_mut())? {
            AppExit::Quit => return Ok(()),
            AppExit::OpenPager(path) => {
                // Caught handlers reset on exec, so the pager receives these signals normally.
                keyboard_signals_default.store(false, Ordering::SeqCst);
                terminal.suspend()?;
                let result = pager
                    .spawn(&path)
                    .and_then(|child| app.wait_for_pager(child));
                terminal.resume()?;
                keyboard_signals_default.store(true, Ordering::SeqCst);
                app.resume_input();
                if let Err(error) = result {
                    app.show_dialog(Dialog::CommandError {
                        command: pager.command.clone(),
                        output: error.to_string(),
                    });
                }
            }
        }
    }
}
