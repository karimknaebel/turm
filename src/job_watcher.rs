use std::path::PathBuf;
use std::sync::LazyLock;
use std::{io::BufRead, process::Command, thread, time::Duration};

use crossbeam::channel::Sender;
use regex::Regex;

// see https://slurm.schedmd.com/sbatch.html#SECTION_%3CB%3Efilename-pattern%3C/B%3E
static PATH_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"%(%|A|a|J|j|N|n|s|t|u|x)").unwrap());

use crate::app::AppMessage;
use crate::app::Job;

struct JobWatcher {
    app: Sender<AppMessage>,
    interval: Duration,
    squeue_args: Vec<String>,
}

pub struct JobWatcherHandle {}

/// Context needed to resolve slurm filename patterns (e.g. `%j`, `%u`) in
/// `stdout`/`stderr` paths into concrete paths.
///
/// See https://slurm.schedmd.com/sbatch.html#SECTION_%3CB%3Efilename-pattern%3C/B%3E
struct FilenameContext<'a> {
    array_master: &'a str,
    array_id: &'a str,
    id: &'a str,
    host: &'a str,
    user: &'a str,
    name: &'a str,
    working_dir: &'a str,
}

impl FilenameContext<'_> {
    fn resolve(&self, path: &str) -> Option<PathBuf> {
        let slurm_no_val = "4294967294";
        let array_id = if self.array_id == "N/A" {
            slurm_no_val
        } else {
            self.array_id
        };

        let path = if path.is_empty() {
            // never happens right now, because `squeue -O stdout` seems to always return something
            if array_id == slurm_no_val {
                PathBuf::from(self.working_dir).join("slurm-%J.out")
            } else {
                PathBuf::from(self.working_dir).join("slurm-%A_%a.out")
            }
            .to_str()
            .unwrap()
            .to_owned()
        } else {
            path.to_owned()
        };

        let resolved = PATH_REGEX.replace_all(&path, |caps: &regex::Captures| {
            match caps.get(0).unwrap().as_str() {
                "%%" => "%",
                "%A" => self.array_master,
                "%a" => array_id,
                "%J" => self.id,
                "%j" => self.id,
                "%N" => self.host.split(',').next().unwrap_or(self.host),
                "%n" => "0",
                "%s" => "batch",
                "%t" => "0",
                "%u" => self.user,
                "%x" => self.name,
                _ => unreachable!(),
            }
            .to_string()
        });

        Some(PathBuf::from(self.working_dir).join(resolved.as_ref())) // works even if `path` is absolute
    }
}

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

        let filename_ctx = FilenameContext {
            array_master: array_job_id,
            array_id: array_task_id,
            id,
            host: node_list,
            user,
            name,
            working_dir,
        };

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
            stdout: filename_ctx.resolve(stdout),
            stderr: filename_ctx.resolve(stderr),
        })
    }

    fn run(&mut self) {
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
            "12345",               // id
            "my-job",              // name
            "RUNNING",             // state
            "alice",               // user
            "1:00:00",             // time
            "4:00:00",             // time_limit
            "2024-01-01T00:00:00", // start_time
            "cpu=4",               // tres
            "gpu",                 // partition
            "node01",              // nodelist
            "/scratch/12345.out",  // stdout
            "/scratch/12345.err",  // stderr
            "/home/alice/job.sh",  // command
            "R",                   // state_compact
            "None",                // reason
            "12345",               // array_job_id
            "N/A",                 // array_task_id
            "node01",              // node_list
            "/home/alice",         // working_dir
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
            "100", // array_job_id
            "3",   // array_task_id — not N/A
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

    fn ctx<'a>() -> FilenameContext<'a> {
        FilenameContext {
            array_master: "100",
            array_id: "2",
            id: "12345",
            host: "node01,node02",
            user: "alice",
            name: "myjob",
            working_dir: "/home/alice",
        }
    }

    #[test]
    fn substitutes_job_id_and_user() {
        let resolved = ctx().resolve("out-%j-%u.log").unwrap();
        assert_eq!(resolved, PathBuf::from("/home/alice/out-12345-alice.log"));
    }

    #[test]
    fn substitutes_first_node_from_list() {
        let resolved = ctx().resolve("%N.out").unwrap();
        assert_eq!(resolved, PathBuf::from("/home/alice/node01.out"));
    }

    #[test]
    fn substitutes_first_node_from_list_single_node() {
        let context = FilenameContext {
            host: "node01",
            ..ctx()
        };
        let resolved = context.resolve("%N.out").unwrap();
        assert_eq!(resolved, PathBuf::from("/home/alice/node01.out"));
    }

    #[test]
    fn na_array_id_uses_sentinel() {
        let context = FilenameContext {
            array_id: "N/A",
            ..ctx()
        };
        let resolved = context.resolve("%a.out").unwrap();
        assert_eq!(resolved, PathBuf::from("/home/alice/4294967294.out"));
    }

    #[test]
    fn substitutes_percent_a_with_real_id() {
        let resolved = ctx().resolve("%a.out").unwrap();
        assert_eq!(resolved, PathBuf::from("/home/alice/2.out"));
    }

    #[test]
    fn substitutes_percent_capital_a() {
        let resolved = ctx().resolve("%A.out").unwrap();
        assert_eq!(resolved, PathBuf::from("/home/alice/100.out"));
    }

    #[test]
    fn substitutes_percent_capital_j() {
        let resolved = ctx().resolve("%J.out").unwrap();
        assert_eq!(resolved, PathBuf::from("/home/alice/12345.out"));
    }

    #[test]
    fn substitutes_percent_x() {
        let resolved = ctx().resolve("%x.out").unwrap();
        assert_eq!(resolved, PathBuf::from("/home/alice/myjob.out"));
    }

    #[test]
    fn absolute_path_is_preserved() {
        let resolved = ctx().resolve("/scratch/%u/%j.out").unwrap();
        assert_eq!(resolved, PathBuf::from("/scratch/alice/12345.out"));
    }

    #[test]
    fn empty_path_falls_back_to_working_dir() {
        let resolved = ctx().resolve("").unwrap();
        assert_eq!(resolved, PathBuf::from("/home/alice/slurm-100_2.out"));
    }

    #[test]
    fn literal_percent_and_misc_specifiers() {
        let resolved = ctx().resolve("%%literal-%n-%s-%t.out").unwrap();
        assert_eq!(
            resolved,
            PathBuf::from("/home/alice/%literal-0-batch-0.out")
        );
    }
}
