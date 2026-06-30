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

    const OUTPUT_SEPARATOR: &'static str = "###turm###";
    const FIELDS: [&'static str; 19] = [
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

    /// Parse a single trimmed line of `squeue --Format` output into a `Job`.
    /// Returns `None` if the line does not contain exactly the expected number of fields.
    fn parse_line(line: &str) -> Option<Job> {
        let parts: Vec<_> = line.split(Self::OUTPUT_SEPARATOR).collect();

        if parts.len() != Self::FIELDS.len() + 1 {
            return None;
        }

        let id = parts[0];
        let name = parts[1];
        let state = parts[2];
        let user = parts[3];
        let time = parts[4];
        let time_limit = parts[5];
        let start_time = parts[6];
        let tres = parts[7];
        let partition = parts[8];
        let nodelist = parts[9];
        let stdout = parts[10];
        let stderr = parts[11];
        let command = parts[12];
        let state_compact = parts[13];
        let reason = parts[14];

        let array_job_id = parts[15];
        let array_task_id = parts[16];
        let node_list = parts[17];
        let working_dir = parts[18];

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
            time_limit: time_limit.to_owned(),
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
    }

    fn run(&mut self) -> Self {
        let output_format = Self::FIELDS
            .map(|s| s.to_owned() + ":" + Self::OUTPUT_SEPARATOR)
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
                .filter_map(|l| Self::parse_line(&l))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an squeue output line. Field order must match `JobWatcher::FIELDS`.
    fn make_line(fields: &[&str]) -> String {
        assert_eq!(
            fields.len(),
            JobWatcher::FIELDS.len(),
            "make_line: wrong number of fields"
        );
        let sep = JobWatcher::OUTPUT_SEPARATOR;
        fields.iter().map(|f| format!("{f}{sep}")).collect()
    }

    #[test]
    fn parse_line_well_formed() {
        let line = make_line(&[
            "12345",             // id
            "my-job",           // name
            "RUNNING",          // state
            "alice",            // user
            "1:00:00",          // time
            "4:00:00",          // time_limit
            "2024-01-01T00:00:00", // start_time
            "cpu=4",            // tres
            "gpu",              // partition
            "node01",           // nodelist
            "/scratch/12345.out", // stdout
            "/scratch/12345.err", // stderr
            "/home/alice/job.sh", // command
            "R",                // state_compact
            "None",             // reason
            "12345",            // array_job_id
            "N/A",              // array_task_id
            "node01",           // node_list
            "/home/alice",      // working_dir
        ]);

        let job = JobWatcher::parse_line(&line).expect("should parse a well-formed line");

        assert_eq!(job.job_id, "12345");
        assert_eq!(job.name, "my-job");
        assert_eq!(job.state, "RUNNING");
        assert_eq!(job.user, "alice");
        assert_eq!(job.state_compact, "R");
        assert_eq!(job.reason, None, "reason 'None' should map to Option::None");
        assert_eq!(job.array_id, "12345");
        assert_eq!(
            job.array_step, None,
            "array_task_id 'N/A' should yield None"
        );
    }

    #[test]
    fn parse_line_non_na_array_task_id() {
        let line = make_line(&[
            "100",
            "array-job",
            "PENDING",
            "bob",
            "0:00",
            "2:00:00",
            "N/A",
            "cpu=1",
            "batch",
            "node02",
            "/out/%A_%a.out",
            "/out/%A_%a.err",
            "/home/bob/run.sh",
            "PD",
            "Priority",
            "100",   // array_job_id
            "3",     // array_task_id — not N/A
            "node02",
            "/home/bob",
        ]);

        let job = JobWatcher::parse_line(&line).expect("should parse");
        assert_eq!(
            job.array_step,
            Some("3".to_owned()),
            "numeric array_task_id should be preserved"
        );
        assert_eq!(job.reason, Some("Priority".to_owned()));
    }

    #[test]
    fn parse_line_too_few_fields_returns_none() {
        let sep = JobWatcher::OUTPUT_SEPARATOR;
        let short_line = format!("1{sep}2{sep}3{sep}4{sep}5{sep}");
        assert!(
            JobWatcher::parse_line(&short_line).is_none(),
            "wrong field count should return None"
        );
    }

    #[test]
    fn parse_line_empty_line_returns_none() {
        assert!(JobWatcher::parse_line("").is_none());
    }

    #[test]
    fn resolve_path_percent_j_substitution() {
        let result = JobWatcher::resolve_path("/out/%j.log", "100", "N/A", "42", "node01", "alice", "myjob", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/out/42.log"));
    }

    #[test]
    fn resolve_path_percent_capital_j_substitution() {
        let result = JobWatcher::resolve_path("/out/%J.log", "100", "N/A", "99", "node01", "alice", "myjob", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/out/99.log"));
    }

    #[test]
    fn resolve_path_percent_u_substitution() {
        let result = JobWatcher::resolve_path("/scratch/%u/out.log", "100", "N/A", "1", "node01", "carol", "job", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/scratch/carol/out.log"));
    }

    #[test]
    fn resolve_path_percent_x_substitution() {
        let result = JobWatcher::resolve_path("/logs/%x.out", "100", "N/A", "5", "node01", "dave", "simulation", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/logs/simulation.out"));
    }

    #[test]
    fn resolve_path_percent_a_substitution() {
        let result = JobWatcher::resolve_path("/out/%a.log", "200", "7", "201", "node01", "alice", "job", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/out/7.log"));
    }

    #[test]
    fn resolve_path_percent_capital_a_substitution() {
        let result = JobWatcher::resolve_path("/out/%A.log", "200", "7", "201", "node01", "alice", "job", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/out/200.log"));
    }

    #[test]
    fn resolve_path_percent_capital_n_single_node() {
        let result = JobWatcher::resolve_path("/logs/%N.out", "1", "N/A", "10", "node01", "alice", "job", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/logs/node01.out"));
    }

    #[test]
    fn resolve_path_percent_capital_n_multi_node_picks_first() {
        let result = JobWatcher::resolve_path(
            "/logs/%N.out",
            "1",
            "N/A",
            "10",
            "node01,node02,node03",
            "alice",
            "job",
            "/work",
        ).unwrap();
        assert_eq!(result, PathBuf::from("/logs/node01.out"));
    }

    #[test]
    fn resolve_path_double_percent_becomes_literal() {
        let result = JobWatcher::resolve_path("/logs/100%%.out", "1", "N/A", "10", "node01", "alice", "job", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/logs/100%.out"));
    }

    #[test]
    fn resolve_path_na_array_id_becomes_sentinel() {
        let result = JobWatcher::resolve_path("/out/%a.log", "100", "N/A", "55", "node01", "alice", "job", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/out/4294967294.log"));
    }

    #[test]
    fn resolve_path_absolute_path_ignores_working_dir() {
        let result = JobWatcher::resolve_path("/abs/path/out.log", "1", "N/A", "1", "node01", "alice", "job", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/abs/path/out.log"));
    }

    #[test]
    fn resolve_path_relative_path_is_joined_onto_working_dir() {
        let result = JobWatcher::resolve_path("logs/out.log", "1", "N/A", "1", "node01", "alice", "job", "/work").unwrap();
        assert_eq!(result, PathBuf::from("/work/logs/out.log"));
    }

    #[test]
    fn resolve_path_multiple_patterns_in_one_path() {
        let result = JobWatcher::resolve_path(
            "/scratch/%u/%x-%j.out",
            "500",
            "N/A",
            "501",
            "node01",
            "eve",
            "sim",
            "/home/eve",
        ).unwrap();
        assert_eq!(result, PathBuf::from("/scratch/eve/sim-501.out"));
    }
}
