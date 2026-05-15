mod app;
mod file_watcher;
mod job_watcher;
mod squeue_args;

use app::App;
use clap::CommandFactory;
use clap::Parser;
use clap::Subcommand;
use clap_complete::{Shell, generate};
use crossbeam::channel::{Sender, unbounded};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
};
use squeue_args::SqueueArgs;
use std::{io, panic, thread};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Refresh rate for the job watcher.
    #[arg(long, value_name = "SECONDS", default_value_t = 2)]
    slurm_refresh: u64,

    /// Refresh rate for the file watcher.
    #[arg(long, value_name = "SECONDS", default_value_t = 2)]
    file_refresh: u64,

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

    let mut terminal_guard = TerminalGuard::new()?;
    run_app(terminal_guard.terminal_mut(), args)
}

fn suspend_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableBracketedPaste,
        DisableMouseCapture,
        Show
    )?;
    Ok(())
}

fn resume_terminal() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;
    Ok(())
}

fn install_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = suspend_terminal();
        default_hook(panic_info);
    }));
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        resume_terminal()?;
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = suspend_terminal();
    }
}

fn input_loop(tx: Sender<std::io::Result<Event>>) {
    while tx.send(event::read()).is_ok() {}
}

fn run_app<B: Backend<Error = io::Error>>(terminal: &mut Terminal<B>, args: Cli) -> io::Result<()> {
    let (input_tx, input_rx) = unbounded();
    let mut app = App::new(
        input_rx,
        args.slurm_refresh,
        args.file_refresh,
        args.squeue_args.to_vec(),
    );
    thread::spawn(move || input_loop(input_tx));
    app.run(terminal)
}
