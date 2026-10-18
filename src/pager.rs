use nix::{
    errno::Errno,
    sys::{
        signal::{SigSet, SigmaskHow, Signal, kill, killpg},
        termios::{SetArg, Termios, tcgetattr, tcsetattr},
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
    unistd::{Pid, getpgrp, getpid, tcgetpgrp, tcsetpgrp},
};
use std::{
    fs::File,
    io,
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command},
};

pub struct Pager {
    pub command: String,
    fallback: bool,
}

impl Pager {
    pub fn new(cli: Option<String>, turm_pager: Option<String>, pager: Option<String>) -> Self {
        match [cli, turm_pager, pager]
            .into_iter()
            .flatten()
            .find(|value| !value.trim().is_empty())
        {
            Some(command) => Self {
                command,
                fallback: false,
            },
            None => Self {
                command: "less".to_string(),
                fallback: true,
            },
        }
    }

    pub fn spawn(&self, path: &Path) -> io::Result<PagerChild> {
        match PagerChild::spawn(pager_command(&self.command, path)?) {
            Err(error) if self.fallback && error.kind() == io::ErrorKind::NotFound => {
                PagerChild::spawn(pager_command("more", path)?)
            }
            result => result,
        }
    }
}

pub struct PagerChild {
    child: Child,
    terminal: PagerTerminal,
}

impl PagerChild {
    fn spawn(mut command: Command) -> io::Result<Self> {
        let terminal = PagerTerminal::new()?;
        let tty = terminal.tty.try_clone()?;
        command.process_group(0);
        // Only async-signal-safe operations are allowed between fork and exec.
        unsafe {
            command.pre_exec(move || foreground(&tty, getpgrp()));
        }
        Ok(Self {
            child: command.spawn()?,
            terminal,
        })
    }

    pub fn wait(self) -> io::Result<()> {
        let group = Pid::from_raw(self.child.id() as i32);
        loop {
            match waitpid(group, Some(WaitPidFlag::WUNTRACED)) {
                Ok(WaitStatus::Exited(_, 0))
                | Ok(WaitStatus::Signaled(_, Signal::SIGINT | Signal::SIGQUIT, _)) => {
                    return Ok(());
                }
                Ok(WaitStatus::Exited(_, code)) => {
                    return Err(io::Error::other(format!("Pager exited with status {code}")));
                }
                Ok(WaitStatus::Signaled(_, signal, _)) => {
                    return Err(io::Error::other(format!("Pager exited with {signal}")));
                }
                Ok(WaitStatus::Stopped(_, _)) => {
                    let attributes = tcgetattr(&self.terminal.tty)?;
                    self.terminal.restore()?;
                    kill(getpid(), Signal::SIGSTOP)?;
                    tcsetattr(&self.terminal.tty, SetArg::TCSANOW, &attributes)?;
                    foreground(&self.terminal.tty, group)?;
                    killpg(group, Signal::SIGCONT)?;
                }
                Err(Errno::EINTR) => continue,
                Err(error) => return Err(error.into()),
                _ => {}
            }
        }
    }
}

impl Drop for PagerChild {
    fn drop(&mut self) {
        // Wrappers such as bat can exit while their pager is still reading the terminal.
        let _ = killpg(Pid::from_raw(self.child.id() as i32), Signal::SIGKILL);
        let _ = self.child.wait();
    }
}

struct PagerTerminal {
    tty: File,
    group: Pid,
    attributes: Termios,
}

impl PagerTerminal {
    fn new() -> io::Result<Self> {
        let tty = File::open("/dev/tty")?;
        Ok(Self {
            group: tcgetpgrp(&tty)?,
            attributes: tcgetattr(&tty)?,
            tty,
        })
    }

    fn restore(&self) -> io::Result<()> {
        foreground(&self.tty, self.group)?;
        tcsetattr(&self.tty, SetArg::TCSANOW, &self.attributes)?;
        Ok(())
    }
}

impl Drop for PagerTerminal {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn foreground(tty: &File, group: Pid) -> io::Result<()> {
    // Changing the foreground group from the background would otherwise stop us.
    let mut signals = SigSet::empty();
    signals.add(Signal::SIGTTOU);
    let previous = signals.thread_swap_mask(SigmaskHow::SIG_BLOCK)?;
    let result = tcsetpgrp(tty, group);
    previous.thread_set_mask()?;
    Ok(result?)
}

fn pager_command(pager: &str, path: &Path) -> io::Result<Command> {
    let words = shell_words::split(pager)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let Some((program, args)) = words.split_first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Empty pager command",
        ));
    };
    let mut command = Command::new(program);
    command.args(args).arg(path.canonicalize()?);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_precedence() {
        for (cli, turm, pager, expected, fallback) in [
            (Some("bat"), Some("less"), Some("more"), "bat", false),
            (None, Some("bat"), Some("more"), "bat", false),
            (None, None, Some("more"), "more", false),
            (Some(" "), Some(""), Some("more"), "more", false),
            (None, None, None, "less", true),
        ] {
            let pager = Pager::new(
                cli.map(str::to_owned),
                turm.map(str::to_owned),
                pager.map(str::to_owned),
            );
            assert_eq!(pager.command, expected);
            assert_eq!(pager.fallback, fallback);
        }
    }

    #[test]
    fn quoted_arguments_are_passed_without_shell_expansion() {
        let command = pager_command(
            "bat --theme 'Monokai Extended' '--title=$(whoami)'",
            Path::new("Cargo.toml"),
        )
        .unwrap();
        assert_eq!(command.get_program(), "bat");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                std::ffi::OsStr::new("--theme"),
                std::ffi::OsStr::new("Monokai Extended"),
                std::ffi::OsStr::new("--title=$(whoami)"),
                Path::new("Cargo.toml").canonicalize().unwrap().as_os_str(),
            ]
        );
    }

    #[test]
    fn malformed_command_is_rejected() {
        assert!(pager_command("bat '", Path::new("Cargo.toml")).is_err());
        assert!(pager_command(" ", Path::new("Cargo.toml")).is_err());
    }
}
