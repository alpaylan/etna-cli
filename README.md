# ETNA

ETNA is an Analysis and Evaluation Platform for benchmarking and analyzing the performance of Property-Based Testing (PBT) tools. It hosts a collection of testing workloads implemented in different languages, allowing users to plug-in their own PBT tools and libraries to compare their performance against others.

ETNA was originally written as a Python library that provided a set of APIs for accessing the workloads and running experiments, the library implementation can be found in the [jwshii/etna](https://github.com/jwshii/etna) repository. For detailed information about the architecture and design, you can check [ETNA.md](./ETNA.md) or read our research papers.

This repository hosts a command line interface (CLI) for ETNA, which allows users to interact with the ETNA platform from the command line. The CLI provides commands to manage experiments, workloads, and results, making it easier to run and analyze benchmarks, detailed information regarding the installation and usage of the CLI can be found in the [CLI.md](CLI.md) file.

You can easly install the ETNA CLI, you can use the following CURL command:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/alpaylan/etna-cli/releases/latest/download/etna-installer.sh | sh             
```

## Coverage

We are currently working on expanding the coverage of ETNA with more workloads and testing tools. Below is a list of the currently supported workloads and tools:

| Language | Testing Tools                              | Workloads                            |
| :------- | :----------------------------------------- | :----------------------------------- |
| Haskell  | QuickCheck, LeanCheck, SmallCheck          | BST, RBT, STLC, System F<:, LuParser |
| Rocq     | QuickChick                                 | BST, RBT, STLC, IFC                  |
| Racket   | RackCheck                                  | BST, RBT, STLC, System F             |
| Rust     | QuickCheck(fork)                           | BST, RBT, STLC                       |
| OCaml    | QCheck, Base_quickcheck(WIP), Crowbar(WIP) | BST, RBT, STLC                       |

## Roadmap

- [ ] Python - Hypothesis (September 1-7th)
- [ ] Rust - Bolero (September 8-14th)
- [ ] Rust - Proptest (September 15-21)
- [ ] Rust - LibAFL (September 22-28)

## Development

### Running tests

```bash
make test              # cargo test --workspace (serialised)
make coverage          # writes lcov.info at the repo root
make coverage-html     # browsable report at target/coverage/html/index.html
make coverage-summary  # percentages only, no artifacts
```

Coverage requires `cargo-llvm-cov` (`cargo install cargo-llvm-cov`) and the
`llvm-tools-preview` rustup component.

### Test environment overrides

Integration tests isolate themselves from the user's real `~/.etna` via two
environment variables, which are also useful when experimenting locally:

- `ETNA_HOME` — override the `~/.etna` location (config, experiments, cache).
- `ETNA_OFFLINE=1` — skip the `git pull` on `.etna_cache` during
  `workload add` and `bash`, useful when running against a pre-populated
  cache without network access.

## Research Papers

ICFP'23: Etna: An Evaluation Platform for Property-Based Testing (Experience Report)

```bibtex
@article{10.1145/3607860,
author = {Shi, Jessica and Keles, Alperen and Goldstein, Harrison and Pierce, Benjamin C. and Lampropoulos, Leonidas},
title = {Etna: An Evaluation Platform for Property-Based Testing (Experience Report)},
year = {2023},
issue_date = {August 2023},
publisher = {Association for Computing Machinery},
address = {New York, NY, USA},
volume = {7},
number = {ICFP},
url = {https://doi.org/10.1145/3607860},
doi = {10.1145/3607860},
journal = {Proc. ACM Program. Lang.},
month = aug,
articleno = {218},
numpages = {17},
keywords = {empirical evaluation, mutation testing, property-based testing}
}
```