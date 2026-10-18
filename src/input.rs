use std::{io, thread};

use crossbeam::channel::{Receiver, Sender, bounded, unbounded};
use crossterm::event::{self, Event, KeyCode};

pub const PAGER_KEY: KeyCode = KeyCode::Char('p');

pub struct Input {
    pub receiver: Receiver<io::Result<Event>>,
    resume: Sender<()>,
}

impl Input {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();
        let (resume, resume_receiver) = bounded(1);
        thread::spawn(move || input_loop(sender, resume_receiver, event::read));
        Self { receiver, resume }
    }

    pub fn resume(&self) {
        self.resume.send(()).unwrap();
    }
}

fn input_loop(
    events: Sender<io::Result<Event>>,
    resume: Receiver<()>,
    mut read: impl FnMut() -> io::Result<Event>,
) {
    loop {
        let event = read();
        let pause = matches!(&event, Ok(Event::Key(key)) if key.code == PAGER_KEY);
        let failed = event.is_err();
        if events.send(event).is_err() || failed {
            return;
        }
        // Wait before reading again so the pager has exclusive access to the terminal.
        if pause && resume.recv().is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use std::time::Duration;

    #[test]
    fn pager_key_stops_reads_until_resumed() {
        let (events, receiver) = unbounded();
        let (resume, resume_receiver) = bounded(1);
        let (read_calls, reads) = unbounded();
        let reader = thread::spawn(move || {
            let mut keys = [KeyCode::Char('j'), PAGER_KEY, KeyCode::Char('k')].into_iter();
            input_loop(events, resume_receiver, || {
                read_calls.send(()).unwrap();
                keys.next()
                    .map(|key| Event::Key(KeyEvent::new(key, KeyModifiers::NONE)))
                    .ok_or_else(|| io::Error::other("end of input"))
            });
        });

        for key in [KeyCode::Char('j'), PAGER_KEY] {
            reads.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(
                receiver
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap()
                    .unwrap(),
                Event::Key(KeyEvent::new(key, KeyModifiers::NONE)),
            );
        }
        assert!(reads.recv_timeout(Duration::from_millis(50)).is_err());
        resume.send(()).unwrap();
        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap(),
            Event::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)),
        );
        assert!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .is_err()
        );
        reader.join().unwrap();
    }

    #[test]
    fn disconnecting_while_paused_stops_the_reader() {
        let (events, receiver) = unbounded();
        let (resume, resume_receiver) = bounded(1);
        let reader = thread::spawn(move || {
            let mut keys = [PAGER_KEY].into_iter();
            input_loop(events, resume_receiver, || {
                Ok(Event::Key(KeyEvent::new(
                    keys.next().unwrap(),
                    KeyModifiers::NONE,
                )))
            });
        });

        receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        drop(resume);
        reader.join().unwrap();
    }
}
