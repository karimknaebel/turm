use std::{
    fmt,
    fs::File,
    io::{self, Read, Seek},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use crossbeam::{
    channel::{Receiver, RecvError, SendError, Sender, unbounded},
    select,
};
use notify::{RecursiveMode, Watcher, event::ModifyKind};

use crate::app::AppMessage;

struct FileReader {
    content_sender: Sender<io::Result<String>>,
    receiver: Receiver<()>,
    file_path: PathBuf,
    interval: Duration,
    content: String,
    pos: u64,
}

struct FileWatcher {
    app: Sender<AppMessage>,
    receiver: Receiver<FileWatcherMessage>,
    file_path: Option<PathBuf>,
    watching: bool, // Whether notify watch was successfully started for file_path
    interval: Duration,
}
pub enum FileWatcherMessage {
    FilePath(Option<PathBuf>),
}

pub struct FileWatcherHandle {
    sender: Sender<FileWatcherMessage>,
    file_path: Option<PathBuf>,
}

pub enum FileWatcherError {
    File(io::Error),
}

impl fmt::Display for FileWatcherError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileWatcherError::File(e) => write!(f, "Read error: {}", e),
        }
    }
}

impl FileWatcher {
    fn new(
        app: Sender<AppMessage>,
        receiver: Receiver<FileWatcherMessage>,
        interval: Duration,
    ) -> Self {
        FileWatcher {
            app,
            receiver,
            file_path: None,
            watching: false,
            interval,
        }
    }

    fn run(&mut self) -> Result<(), RecvError> {
        let (watch_sender, watch_receiver) = unbounded::<()>();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let event = res.unwrap();
            if let notify::EventKind::Modify(ModifyKind::Data(_)) = event.kind {
                watch_sender.send(()).unwrap();
            }
        })
        .unwrap();

        let (mut _content_sender, mut _content_receiver) = unbounded::<io::Result<String>>();
        let (mut _watch_sender, mut _watch_receiver) = unbounded::<()>();
        loop {
            select! {
                recv(self.receiver) -> msg => {
                    match msg? {
                        FileWatcherMessage::FilePath(file_path) => {
                            (_content_sender, _content_receiver) = unbounded();
                            (_watch_sender, _watch_receiver) = unbounded::<()>();

                            if self.watching {
                                let p = self.file_path.as_ref().expect("Inconsistent state");
                                watcher.unwatch(p).unwrap_or_else(|_| panic!("Failed to unwatch {:?}", p));
                            }
                            self.file_path = None;
                            self.watching = false;

                            if let Some(p) = file_path {
                                self.file_path = Some(p.clone());

                                let interval = self.interval;
                                thread::spawn({
                                    let p = p.clone();
                                    move || FileReader::new(_content_sender, _watch_receiver, p, interval).run()
                                });

                                self.watching = watcher.watch(Path::new(&p), RecursiveMode::NonRecursive).is_ok();
                            } else {
                                _content_sender.send(Ok("".to_string())).unwrap();
                            }
                        }
                    }
                }
                recv(watch_receiver) -> _ => { _watch_sender.send(()).unwrap(); }
                recv(_content_receiver) -> msg => {
                    let res = msg.unwrap();
                    // If we don't have a file watch yet but the file now reads OK, try enabling watch
                    if !self.watching
                        && let (Ok(_), Some(p)) = (&res, &self.file_path)
                    {
                        self.watching = watcher.watch(Path::new(p), RecursiveMode::NonRecursive).is_ok();
                    }
                    self.app
                        .send(AppMessage::JobOutput(res.map_err(FileWatcherError::File)))
                        .unwrap();
                }
            }
        }
    }
}

impl FileReader {
    fn new(
        content_sender: Sender<io::Result<String>>,
        receiver: Receiver<()>,
        file_path: PathBuf,
        interval: Duration,
    ) -> Self {
        FileReader {
            content_sender,
            receiver,
            file_path,
            interval,
            content: "".to_string(),
            pos: 0,
        }
    }

    fn run(&mut self) -> Result<(), ()> {
        loop {
            self.update().map_err(|_| ())?;
            select! {
                recv(self.receiver) -> msg => {
                    msg.map_err(|_| ())?;
                }
                // in case the file watcher doesn't work (e.g. network mounted fs)
                default(self.interval) => {}
            }
        }
    }

    fn update(&mut self) -> Result<(), SendError<io::Result<String>>> {
        let s = File::open(&self.file_path).and_then(|mut f| {
            // avoid reading the whole file every time
            self.pos = f.seek(io::SeekFrom::Start(self.pos))?;
            self.pos += f.read_to_string(&mut self.content)? as u64;
            Ok(self.content.clone())
        });
        // let s = fs::read_to_string(&self.file_path); // alternative: always read the whole file
        self.content_sender.send(s)
    }
}

impl FileWatcherHandle {
    pub fn new(app: Sender<AppMessage>, interval: Duration) -> Self {
        let (sender, receiver) = unbounded();
        let mut actor = FileWatcher::new(app, receiver, interval);
        thread::spawn(move || actor.run());

        Self {
            sender,
            file_path: None,
        }
    }

    pub fn set_file_path(&mut self, file_path: Option<PathBuf>) {
        if self.file_path != file_path {
            self.file_path = file_path.clone();
            self.sender
                .send(FileWatcherMessage::FilePath(file_path))
                .unwrap();
        }
    }
}
