# turm

[![image](https://img.shields.io/pypi/v/turm.svg)](https://pypi.python.org/pypi/turm)
[![image](https://img.shields.io/crates/v/turm.svg)](https://crates.io/crates/turm)
[![Conda Version](https://img.shields.io/conda/vn/conda-forge/turm.svg)](https://anaconda.org/conda-forge/turm)

A TUI for [Slurm](https://slurm.schedmd.com/), which provides a convenient way to manage your cluster jobs.

<img alt="turm demo" src="https://github.com/user-attachments/assets/7daade50-def3-4bf8-bf12-df311438094e" width="100%" />

`turm` accepts the same options as `squeue` (see [man squeue](https://slurm.schedmd.com/squeue.html#SECTION_OPTIONS)). Use `turm --help` to get a list of all available options. For example, to show only your own jobs, sorted by descending job ID, including all job states (i.e., including completed and failed jobs):
```shell
turm --me --sort=-id --states=ALL
```

## Installation

`turm` is available on [PyPI](https://pypi.org/project/turm/), [crates.io](https://crates.io/crates/turm), and [conda-forge](https://github.com/conda-forge/turm-feedstock):

```shell
# With uv.
uv tool install turm

# With pip.
pip install turm

# With cargo.
cargo install turm

# With pixi.
pixi global install turm

# With conda.
conda install --channel conda-forge turm

# With wget. Make sure ~/.local/bin is in your $PATH.
wget https://github.com/karimknaebel/turm/releases/latest/download/turm-x86_64-unknown-linux-musl.tar.gz -O - | tar -xz -C ~/.local/bin/
```

The [release page](https://github.com/karimknaebel/turm/releases) also contains precompiled binaries for Linux.

### Shell Completion (optional)

#### Bash

In your `.bashrc`, add the following line:
```bash
eval "$(turm completion bash)"
```

#### Zsh

In your `.zshrc`, add the following line:
```zsh
eval "$(turm completion zsh)"
```

#### Fish

In your `config.fish` or in a separate `completions/turm.fish` file, add the following line:
```fish
turm completion fish | source
```

## How it works

`turm` obtains information about jobs by parsing the output of `squeue`.
The reason for this is that `squeue` is available on all Slurm clusters, and running it periodically is not too expensive for the Slurm controller ( particularly when [filtering by user](https://slurm.schedmd.com/squeue.html#OPT_user)).
In contrast, Slurm's C API is unstable, and Slurm's REST API is not always available and can be costly for the Slurm controller.
Another advantage is that we get free support for the exact same CLI flags as `squeue`, which users are already familiar with, for filtering and sorting the jobs.

### Resource usage

TL;DR: `turm` ≈ `watch -n2 squeue` + `tail -f slurm-log.out`

Special care has been taken to ensure that `turm` is as lightweight as possible in terms of its impact on the Slurm controller and its file I/O operations.
The job queue is updated every two seconds by running `squeue`.
When there are many jobs in the queue, it is advisable to specify a single user to reduce the load on the Slurm controller (see [squeue --user](https://slurm.schedmd.com/squeue.html#OPT_user)).
`turm` updates the currently displayed log file on every inotify modify notification, and it only reads the newly appended lines after the initial read.
However, since inotify notifications are not supported for remote file systems, such as NFS, `turm` also polls the file for newly appended bytes every two seconds.

## Development without Slurm

For local UI testing, this repository includes a mock `squeue` and `scancel`:

```shell
direnv allow
cargo run -- --me
```

The `.envrc` prepends `scripts/mock-slurm/bin` to `PATH`.
The mock `squeue` reads/writes files in `scripts/mock-slurm/logs`, so you can test log rendering and refresh behavior without a Slurm install.

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=karimknaebel/turm&type=Date)](https://www.star-history.com/#karimknaebel/turm&Date)
