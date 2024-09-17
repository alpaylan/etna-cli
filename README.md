# ETNA CLI

## Description

ETNA CLI is a command line interface that allows you to interact with the ETNA Benchmarking and Analysis Platform. It provides a set of commands to manage your experiments, and results.

## Installation

To install the ETNA CLI, you can use the following command:

```bash
cargo install etna-cli
```

## Usage

To get started, you can use the `etna-cli --help` command to see the list of available commands.

```bash
etna-cli --help
```

The commands are organized in the following categories:

- `experiment`: Create, delete, move experiments
- `workload`: Create, delete, move workloads within an experiment
- `config`: Manage the global ETNA configuration
- `setup`: Setup the ETNA CLI
