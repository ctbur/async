# async

`async` is a tool to run shell commands in parallel and is designed to be able to quickly parallelize shell scripts with minimal changes.
It was inspired by [GNU Parallel](https://www.gnu.org/software/parallel/), with the main difference being that `async` retains state between commands by running a server in the background.


## Usage

All information about the command line interface is available using `async --help`. Below is an example on how to use `async` to parallelize commands:

```bash
#!/bin/bash
S="/tmp/example_socket"

async -s="$S" server --start

for i in {1..20}; do
    # prints command output to stdout
    async -s="$S" cmd -- bash -c "sleep 1 && echo test $i"
done

# wait until all commands are finished
async -s="$S" wait

# configure the server to run four commands in parallel
async -s="$S" server -j4

mkdir "/tmp/ex_dir"
for i in {21..40}; do
    # redirects command output to /tmp/ex_dir/file*
    async -s="$S" cmd -o "/tmp/ex_dir/file$i" -- bash -c "sleep 1 && echo test $i"
done

async -s="$S" wait

# stops server
async -s="$S" server --stop
```

If you encounter an unexpected error or crash, please turn on logging by setting the environment variable `RUST_LOG=debug` and open an issue on the Github repository.


## Installation

`async` can be installed using cargo with `cargo install async-exec` or from the AUR.
