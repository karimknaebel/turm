use std::path::PathBuf;
use std::{io::BufRead, process::Command, thread, time::Duration};

use crossbeam::channel::Sender;
use regex::Regex;

use crate::app::AppMessage;
use crate::app::Job;

struct JobWatcher {
    app: Sender<AppMessage>,
    interval: Duration,
    squeue_args: Vec<String>,
}

pub struct JobWatcherHandle {}

impl JobWatcher {
    fn new(app: Sender<AppMessage>, interval: Duration, squeue_args: Vec<String>) -> Self {
        Self {
            app,
            interval,
            squeue_args,
        }
    }

    fn run(&mut self) -> Self {
        let output_separator = "###turm###";
        // Order must match the slice pattern destructuring `parts` below field-for-field.
        let fields = [
            "jobid",
            "name",
            "state",
            "username",
            "timeused",
            "timelimit",
            "StartTime",
            "tres-alloc",
            "partition",
            "nodelist",
            "stdout",
            "stderr",
            "command",
            "statecompact",
            "reason",
            "ArrayJobID",  // %A
            "ArrayTaskID", // %a
            "NodeList",    // %N
            "WorkDir",     // for fallback
        ];
        let output_format = fields
            .map(|s| s.to_owned() + ":" + output_separator)
            .join(",");

        loop {
            let jobs: Vec<Job> = Command::new("squeue")
                .args(&self.squeue_args)
                .arg("--array")
                .arg("--noheader")
                .arg("--Format")
                .arg(&output_format)
                .output()
                .expect("failed to execute process")
                .stdout
                .lines()
                .map(|l| l.unwrap().trim().to_string())
                .filter_map(|l| {
                    let parts: Vec<_> = l.split(output_separator).collect();

                    let [
                        id,
                        name,
                        state,
                        user,
                        time,
                        time_limit,
                        start_time,
                        tres,
                        partition,
                        nodelist,
                        stdout,
                        stderr,
                        command,
                        state_compact,
                        reason,
                        array_job_id,
                        array_task_id,
                        node_list,
                        working_dir,
                        _trailing,
                    ] = parts.as_slice()
                    else {
                        return None;
                    };

                    Some(Job {
                        job_id: id.to_string(),
                        array_id: array_job_id.to_string(),
                        array_step: match *array_task_id {
                            "N/A" => None,
                            _ => Some(array_task_id.to_string()),
                        },
                        name: name.to_string(),
                        state: state.to_string(),
                        state_compact: state_compact.to_string(),
                        reason: if *reason == "None" {
                            None
                        } else {
                            Some(reason.to_string())
                        },
                        user: user.to_string(),
                        time: time.to_string(),
                        time_limit: time_limit.to_string(),
                        start_time: start_time.to_string(),
                        tres: tres.to_string(),
                        partition: partition.to_string(),
                        nodelist: nodelist.to_string(),
                        command: command.to_string(),
                        stdout: Self::resolve_path(
                            stdout,
                            array_job_id,
                            array_task_id,
                            id,
                            node_list,
                            user,
                            name,
                            working_dir,
                        ),
                        stderr: Self::resolve_path(
                            stderr,
                            array_job_id,
                            array_task_id,
                            id,
                            node_list,
                            user,
                            name,
                            working_dir,
                        ), // TODO fill all fields
                    })
                })
                .collect();
            self.app.send(AppMessage::Jobs(jobs)).unwrap();
            thread::sleep(self.interval);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_path(
        path: &str,
        array_master: &str,
        array_id: &str,
        id: &str,
        host: &str,
        user: &str,
        name: &str,
        working_dir: &str,
    ) -> Option<PathBuf> {
        // see https://slurm.schedmd.com/sbatch.html#SECTION_%3CB%3Efilename-pattern%3C/B%3E
        lazy_static::lazy_static! {
            static ref RE: Regex = Regex::new(r"%(%|A|a|J|j|N|n|s|t|u|x)").unwrap();
        }

        let mut path = path.to_owned();
        let slurm_no_val = "4294967294";
        let array_id = if array_id == "N/A" {
            slurm_no_val
        } else {
            array_id
        };

        if path.is_empty() {
            // never happens right now, because `squeue -O stdout` seems to always return something
            path = if array_id == slurm_no_val {
                PathBuf::from(working_dir).join("slurm-%J.out")
            } else {
                PathBuf::from(working_dir).join("slurm-%A_%a.out")
            }
            .to_str()
            .unwrap()
            .to_owned();
        };

        for cap in RE
            .captures_iter(&path.clone())
            .collect::<Vec<_>>() // TODO: this is stupid, there has to be a better way to reverse the captures...
            .iter()
            .rev()
        {
            let m = cap.get(0).unwrap();
            let replacement = match m.as_str() {
                "%%" => "%",
                "%A" => array_master,
                "%a" => array_id,
                "%J" => id,
                "%j" => id,
                "%N" => host.split(',').next().unwrap_or(host),
                "%n" => "0",
                "%s" => "batch",
                "%t" => "0",
                "%u" => user,
                "%x" => name,
                _ => unreachable!(),
            };

            path.replace_range(m.range(), replacement);
        }

        Some(PathBuf::from(working_dir).join(path)) // works even if `path` is absolute
    }
}

impl JobWatcherHandle {
    pub fn new(app: Sender<AppMessage>, interval: Duration, squeue_args: Vec<String>) -> Self {
        let mut actor = JobWatcher::new(app, interval, squeue_args);
        thread::spawn(move || actor.run());

        Self {}
    }
}
