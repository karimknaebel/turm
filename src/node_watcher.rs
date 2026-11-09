use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{io::BufRead, process::Command, thread, time::Duration};

use crossbeam::channel::Sender;

use crate::app::AppMessage;

pub struct Node {
    pub name: String,
    pub state: String,
    pub cpus_alloc: u64,
    pub cpus_total: u64,
    pub cpu_load: String,
    pub mem_alloc: u64, // MiB
    pub mem_total: u64, // MiB
}

pub struct NodeWatcherHandle {
    pub active: Arc<AtomicBool>,
}

const OUTPUT_SEPARATOR: &str = "###turm###";
const FIELDS: [&str; 6] = [
    "NodeHost",
    "StateCompact",
    "CPUsState", // allocated/idle/other/total
    "CPUsLoad",
    "AllocMem",
    "Memory",
];

fn parse_line(line: &str) -> Option<Node> {
    let parts: Vec<_> = line.split(OUTPUT_SEPARATOR).map(str::trim).collect();
    if parts.len() != FIELDS.len() + 1 {
        return None;
    }
    let cpus: Vec<_> = parts[2].split('/').collect();
    Some(Node {
        name: parts[0].to_owned(),
        state: parts[1].to_owned(),
        cpus_alloc: cpus.first()?.parse().ok()?,
        cpus_total: cpus.get(3)?.parse().ok()?,
        cpu_load: parts[3].to_owned(),
        mem_alloc: parts[4].parse().ok()?,
        mem_total: parts[5].parse().ok()?,
    })
}

pub fn percent(part: u64, total: u64) -> u64 {
    (part * 100).checked_div(total).unwrap_or(0)
}

/// Cluster summary: node count per state, and total CPU and memory allocation.
pub fn summary(nodes: &[Node]) -> (String, String) {
    let mut states = BTreeMap::new();
    for n in nodes {
        *states.entry(n.state.as_str()).or_insert(0) += 1;
    }
    let states = states
        .iter()
        .map(|(s, c)| format!("{s} {c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let (ca, ct, ma, mt) = nodes.iter().fold((0, 0, 0, 0), |(ca, ct, ma, mt), n| {
        (
            ca + n.cpus_alloc,
            ct + n.cpus_total,
            ma + n.mem_alloc,
            mt + n.mem_total,
        )
    });
    let totals = format!(
        "CPU {ca}/{ct} {}% │ Mem {}/{}G {}%",
        percent(ca, ct),
        ma / 1024,
        mt / 1024,
        percent(ma, mt),
    );
    (states, totals)
}

impl NodeWatcherHandle {
    pub fn new(app: Sender<AppMessage>, interval: Duration) -> Self {
        let active = Arc::new(AtomicBool::new(false));
        let watcher_active = active.clone();
        let format = FIELDS.map(|f| format!("{f}:{OUTPUT_SEPARATOR}")).join(",");
        thread::spawn(move || {
            loop {
                // only poll slurmctld while the node view is open
                if !watcher_active.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(200));
                    continue;
                }
                // shortcut: sinfo failure shows an empty list, surface the error if users get confused
                let mut nodes: Vec<Node> = Command::new("sinfo")
                    .args(["--Node", "--noheader", "--Format", &format])
                    .output()
                    .map(|o| {
                        o.stdout
                            .lines()
                            .map_while(Result::ok)
                            .filter_map(|l| parse_line(&l))
                            .collect()
                    })
                    .unwrap_or_default();
                // `sinfo --Node` repeats a node once per partition
                nodes.sort_by(|a, b| a.name.cmp(&b.name));
                nodes.dedup_by(|a, b| a.name == b.name);
                if app.send(AppMessage::Nodes(nodes)).is_err() {
                    return;
                }
                thread::sleep(interval);
            }
        });
        Self { active }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(fields: &[&str]) -> String {
        fields
            .iter()
            .map(|f| format!("{f}{OUTPUT_SEPARATOR}"))
            .collect()
    }

    #[test]
    fn parse_and_summarize() {
        let a = parse_line(&line(&["n1", "mix", "4/4/0/8", "3.50", "2048", "8192"])).unwrap();
        let b = parse_line(&line(&["n2  ", "down*", "0/0/16/16", "N/A", "0", "8192"])).unwrap();
        assert_eq!((a.cpus_alloc, a.cpus_total, a.mem_total), (4, 8, 8192));
        assert_eq!(b.name, "n2");
        assert!(parse_line(&line(&["n3", "idle", "bad", "0", "0", "0"])).is_none());
        assert!(parse_line("garbage").is_none());
        let (states, totals) = summary(&[a, b]);
        assert_eq!(states, "down* 1, mix 1");
        assert_eq!(totals, "CPU 4/24 16% │ Mem 2/16G 12%");
        assert_eq!(summary(&[]).1, "CPU 0/0 0% │ Mem 0/0G 0%");
    }
}
