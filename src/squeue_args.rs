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
    #[arg(short, long, value_name = "JOBID")]
    job: Option<String>,

    /// |squeue arg| Report information only about jobs on the local cluster. Overrides `--federation`.
    #[arg(long)]
    local: bool,

    /// |squeue arg| Comma separated list of license names to view.
    #[arg(short = 'L', long)]
    licenses: Option<String>,

    /// |squeue arg| Cluster to issue commands to. Default is current cluster. Cluster with no name will reset to default. Implies `--local`.
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
    #[arg(short, long)]
    step: Option<String>,

    /// |squeue arg| Comma separated list of fields to sort on.
    #[arg(short = 'S', long, value_name = "FIELDS")]
    sort: Option<String>,

    /// |squeue arg| Comma separated list of states to view, default is pending and running, `--states=all` reports all states.
    #[arg(short = 't', long)]
    states: Option<String>,

    /// |squeue arg| Comma separated list of users to view.
    #[arg(short = 'u', long)]
    user: Option<String>,

    /// |squeue arg| List of nodes to view, default is all nodes.
    #[arg(short = 'w', long, value_name = "NODES")]
    nodelist: Option<String>,
}

/// A single squeue flag, either taking a value (`--name=value`) or acting as
/// a boolean switch (`--name`).
enum Flag<'a> {
    Value(&'static str, Option<&'a String>),
    Bool(&'static str, bool),
}

impl SqueueArgs {
    pub fn to_vec(&self) -> Vec<String> {
        // Table describing every squeue flag in the same order as the struct
        // fields above, so the two stay easy to keep in sync.
        let flags = [
            Flag::Value("account", self.account.as_ref()),
            Flag::Bool("all", self.all),
            Flag::Bool("federation", self.federation),
            Flag::Bool("hide", self.hide),
            Flag::Value("job", self.job.as_ref()),
            Flag::Bool("local", self.local),
            Flag::Value("licenses", self.licenses.as_ref()),
            Flag::Value("clusters", self.clusters.as_ref()),
            Flag::Bool("me", self.me),
            Flag::Value("name", self.name.as_ref()),
            Flag::Bool("noconvert", self.noconvert),
            Flag::Value("partition", self.partition.as_ref()),
            Flag::Value("qos", self.qos.as_ref()),
            Flag::Value("reservation", self.reservation.as_ref()),
            Flag::Bool("sibling", self.sibling),
            Flag::Value("step", self.step.as_ref()),
            Flag::Value("sort", self.sort.as_ref()),
            Flag::Value("states", self.states.as_ref()),
            Flag::Value("user", self.user.as_ref()),
            Flag::Value("nodelist", self.nodelist.as_ref()),
        ];

        let mut args = Vec::new();
        for flag in flags {
            match flag {
                Flag::Value(name, Some(value)) => args.push(format!("--{name}={value}")),
                Flag::Value(_, None) => {}
                Flag::Bool(name, true) => args.push(format!("--{name}")),
                Flag::Bool(_, false) => {}
            }
        }
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_vec_formats_value_and_bool_flags_in_declaration_order() {
        let args = SqueueArgs {
            account: Some("myaccount".to_string()),
            all: false,
            federation: false,
            hide: false,
            job: Some("123,456".to_string()),
            local: false,
            licenses: None,
            clusters: None,
            me: true,
            name: None,
            noconvert: false,
            partition: None,
            qos: None,
            reservation: None,
            sibling: true,
            step: None,
            sort: None,
            states: None,
            user: None,
            nodelist: None,
        };

        assert_eq!(
            args.to_vec(),
            vec![
                "--account=myaccount".to_string(),
                "--job=123,456".to_string(),
                "--me".to_string(),
                "--sibling".to_string(),
            ]
        );
    }
}
