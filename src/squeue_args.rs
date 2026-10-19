use clap::Args;
/// Doc comment
#[derive(Args, Debug)]
pub struct SqueueArgs {
    /// |squeue arg| Comma separated list of accounts to view, default is all accounts.
    #[arg(short = 'A', long)]
    account: Option<String>,

    /// |squeue arg| Display jobs in hidden partitions.
    #[arg(short, long)]
    all: bool,

    /// |squeue arg| Report federated information if a member of one.
    #[arg(long)]
    federation: bool,

    /// |squeue arg| Do not display jobs in hidden partitions.
    #[arg(long)]
    hide: bool,

    /// |squeue arg| Comma separated list of jobs IDs to view, default is all.
    ///
    /// Job ID syntax: https://slurm.schedmd.com/squeue.html#OPT_jobs
    #[arg(short, long, value_name = "JOBID")]
    job: Option<String>,

    /// |squeue arg| Report information only about jobs on the local cluster. Overrides `--federation`.
    #[arg(long)]
    local: bool,

    /// |squeue arg| Comma separated list of license names to view.
    #[arg(short = 'L', long)]
    licenses: Option<String>,

    /// |squeue arg| Cluster to issue commands to. Default is current cluster. Cluster with no name will reset to default. Implies `--local`.
    ///
    /// Cluster selection: https://slurm.schedmd.com/squeue.html#OPT_clusters
    #[arg(short = 'M', long)]
    clusters: Option<String>,

    /// |squeue arg| Equivalent to `--user=<my username>`.
    #[arg(long)]
    me: bool,

    /// |squeue arg| Comma separated list of job names to view.
    #[arg(short = 'n', long)]
    name: Option<String>,

    /// |squeue arg| Don't convert units from their original type (e.g. 2048M won't be converted to 2G).
    #[arg(long)]
    noconvert: bool,

    /// |squeue arg| Comma separated list of partitions to view, default is all partitions.
    #[arg(short, long)]
    partition: Option<String>,

    /// |squeue arg| Comma separated list of qos's to view, default is all qos's.
    #[arg(short, long)]
    qos: Option<String>,

    /// |squeue arg| Reservation to view, default is all.
    #[arg(short = 'R', long)]
    reservation: Option<String>,

    /// |squeue arg| Report information about all sibling jobs on a federated cluster. Implies --federation.
    #[arg(long)]
    sibling: bool,

    /// |squeue arg| Comma separated list of job steps to view, default is all.
    ///
    /// Step ID syntax: https://slurm.schedmd.com/squeue.html#OPT_steps
    #[arg(short, long)]
    step: Option<String>,

    /// |squeue arg| Comma separated Slurm sort keys (e.g. i for job ID, -M for descending elapsed time).
    ///
    /// Sort keys and ordering: https://slurm.schedmd.com/squeue.html#OPT_sort
    #[arg(short = 'S', long, value_name = "FIELDS")]
    sort: Option<String>,

    /// |squeue arg| Comma separated states to view, default is pending, running and completing. Accepts names or codes (e.g. RUNNING or R); `--states=all` reports all states.
    ///
    /// State names and codes: https://slurm.schedmd.com/squeue.html#SECTION_JOB-STATE-CODES
    #[arg(short = 't', long)]
    states: Option<String>,

    /// |squeue arg| Comma separated list of users to view.
    #[arg(short = 'u', long)]
    user: Option<String>,

    /// |squeue arg| List of nodes to view, default is all nodes.
    ///
    /// Node selection: https://slurm.schedmd.com/squeue.html#OPT_nodelist
    #[arg(short = 'w', long, value_name = "NODES")]
    nodelist: Option<String>,
}

impl SqueueArgs {
    pub fn to_vec(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(account) = &self.account {
            args.push(format!("--account={}", account));
        }
        if self.all {
            args.push("--all".to_string());
        }
        if self.federation {
            args.push("--federation".to_string());
        }
        if self.hide {
            args.push("--hide".to_string());
        }
        if let Some(job) = &self.job {
            args.push(format!("--job={}", job));
        }
        if self.local {
            args.push("--local".to_string());
        }
        if let Some(licenses) = &self.licenses {
            args.push(format!("--licenses={}", licenses));
        }
        if let Some(clusters) = &self.clusters {
            args.push(format!("--clusters={}", clusters));
        }
        if self.me {
            args.push("--me".to_string());
        }
        if let Some(name) = &self.name {
            args.push(format!("--name={}", name));
        }
        if self.noconvert {
            args.push("--noconvert".to_string());
        }
        if let Some(partition) = &self.partition {
            args.push(format!("--partition={}", partition));
        }
        if let Some(qos) = &self.qos {
            args.push(format!("--qos={}", qos));
        }
        if let Some(reservation) = &self.reservation {
            args.push(format!("--reservation={}", reservation));
        }
        if self.sibling {
            args.push("--sibling".to_string());
        }
        if let Some(step) = &self.step {
            args.push(format!("--step={}", step));
        }
        if let Some(sort) = &self.sort {
            args.push(format!("--sort={}", sort));
        }
        if let Some(states) = &self.states {
            args.push(format!("--states={}", states));
        }
        if let Some(user) = &self.user {
            args.push(format!("--user={}", user));
        }
        if let Some(nodelist) = &self.nodelist {
            args.push(format!("--nodelist={}", nodelist));
        }
        args
    }
}
