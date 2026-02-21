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
        let fields = [
            "jobid",
            "name",
            "state",
            "username",
            "timeused",
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

        let fields_sacct = [
            "jobid",
            "jobname",
            "state",
            "user",
            "Elapsed",
            "AllocTRES",
            "Partition",
            "NodeList",
            "SubmitLine",
            "Reason",
            "WorkDir",
        ];
        let output_format_sacct = fields_sacct.join(",");

        loop {
            let mut jobs_sacct: Vec<Job> = Command::new("sacct")
                // .arg("--starttime ") todo make an option
                .arg("-P")
                .arg("--noheader")
                .arg("--delimiter=###turm###")
                .arg("--format")
                .arg(&output_format_sacct)
                .output()
                .expect("failed to execute sacct process")
                .stdout
                .lines()
                .map(|l| l.unwrap().trim().to_string())
                .filter_map(|l| {
                    let parts: Vec<_> = l.split(output_separator).collect();

                    if parts.len() != 11 {
                        return None;
                    }

                    // jobid 0,jobname 1,state 2,user 3,Elapsed 4,AllocTRES5, Partition 6,NodeList 7, SubmitLine 8,Reason 9,WorkDir 10
                    let id = parts[0];
                    let name = parts[1];
                    let state = parts[2];

                    // remove the .batch and .extern jobs from sacct
                    if id.contains(".") {
                        return None;
                    }
                    // do not print running jobs, handeled by squue
                    if state == "RUNNING" {
                        return None;
                    }

                    let user = parts[3];
                    let time = parts[4];
                    let tres = parts[5];
                    let partition = parts[6];
                    let nodelist = parts[7];
                    // let stdout = parts[8];
                    // let stderr = parts[9];
                    let command = parts[8];
                    let state_compact = &parts[2][0..1];
                    let reason = parts[9];

                    let array_job_id =  "N/A";
                    let array_task_id =  "N/A";
                    let node_list = parts[7];
                    let working_dir = parts[10];

                    Some(Job {
                        job_id: id.to_owned(),
                        array_id: array_job_id.to_owned(),
                        array_step: match array_task_id {
                            "N/A" => None,
                            _ => Some(array_task_id.to_owned()),
                        },
                        name: name.to_owned(),
                        state: state.to_owned(),
                        state_compact: state_compact.to_owned(),
                        reason: if reason == "None" {
                            None
                        } else {
                            Some(reason.to_owned())
                        },
                        user: user.to_owned(),
                        time: time.to_owned(),
                        tres: tres.to_owned(),
                        partition: partition.to_owned(),
                        nodelist: nodelist.to_owned(),
                        command: command.to_owned(),
                        stdout: Self::resolve_path(
                            "",
                            array_job_id,
                            array_task_id,
                            id,
                            node_list,
                            user,
                            name,
                            working_dir,
                        ),
                        stderr: Self::resolve_path(
                            "",
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


            let mut jobs_squeue: Vec<Job> =  Command::new("squeue")
                .args(&self.squeue_args)
                .arg("--array")
                .arg("--noheader")
                .arg("--Format")
                .arg(&output_format)
                .output().expect("failed to execute squeue process")
                .stdout
                .lines()
                .map(|l| l.unwrap().trim().to_string())
                .filter_map(|l| {
                    let parts: Vec<_> = l.split(output_separator).collect();

                    if parts.len() != fields.len() + 1 {
                        return None;
                    }

                    let id = parts[0];
                    let name = parts[1];
                    let state = parts[2];
                    let user = parts[3];
                    let time = parts[4];
                    let start_time = parts[5];
                    let tres = parts[6];
                    let partition = parts[7];
                    let nodelist = parts[8];
                    let stdout = parts[9];
                    let stderr = parts[10];
                    let command = parts[11];
                    let state_compact = parts[12];
                    let reason = parts[13];

                    let array_job_id = parts[14];
                    let array_task_id = parts[15];
                    let node_list = parts[16];
                    let working_dir = parts[17];

                    Some(Job {
                        job_id: id.to_owned(),
                        array_id: array_job_id.to_owned(),
                        array_step: match array_task_id {
                            "N/A" => None,
                            _ => Some(array_task_id.to_owned()),
                        },
                        name: name.to_owned(),
                        state: state.to_owned(),
                        state_compact: state_compact.to_owned(),
                        reason: if reason == "None" {
                            None
                        } else {
                            Some(reason.to_owned())
                        },
                        user: user.to_owned(),
                        time: time.to_owned(),
                        start_time: start_time.to_owned(),
                        tres: tres.to_owned(),
                        partition: partition.to_owned(),
                        nodelist: nodelist.to_owned(),
                        command: command.to_owned(),
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

            jobs_squeue.append(&mut jobs_sacct);
            self.app.send(AppMessage::Jobs(jobs_squeue)).unwrap();
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
