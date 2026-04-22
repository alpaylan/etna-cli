# ETNA

ETNA is an Analysis and Evaluation Platform for benchmarking and analyzing the performance of Property-Based Testing (PBT) tools. It hosts a collection of testing workloads implemented in different languages, allowing users to plug-in their own PBT tools and libraries to compare their performance against others.

ETNA was originally written as a Python library that provided a set of APIs for accessing the workloads and running experiments, the library implementation can be found in the [jwshii/etna](https://github.com/jwshii/etna) repository. For detailed information about the architecture and design, you can check [ETNA.md](./ETNA.md) or read our research papers.

This repository hosts a command line interface (CLI) for ETNA, which allows users to interact with the ETNA platform from the command line. The CLI provides commands to manage experiments, workloads, and results, making it easier to run and analyze benchmarks, detailed information regarding the installation and usage of the CLI can be found in the [CLI.md](CLI.md) file.

You can easly install the ETNA CLI, you can use the following CURL command:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/alpaylan/etna-cli/releases/latest/download/etna-installer.sh | sh             
```

## Coverage

We are currently working on expanding the coverage of ETNA with more workloads and testing tools. Each workload lives in its own repository and is added to an experiment via `etna workload add <url>`:

| Language | Testing Tools                              | Workloads                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| :------- | :----------------------------------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Haskell  | QuickCheck, LeanCheck, SmallCheck          | [bst-haskell](https://github.com/alpaylan/etna-haskell-bst), [rbt-haskell](https://github.com/alpaylan/etna-haskell-rbt), [stlc-haskell](https://github.com/alpaylan/etna-haskell-stlc), [fsub-haskell](https://github.com/alpaylan/etna-haskell-fsub), [luparser-haskell](https://github.com/alpaylan/etna-haskell-luparser)                                                                                                                                                                                                                                                                                                          |
| Rocq     | QuickChick, PropLang                       | [bst-rocq](https://github.com/alpaylan/etna-rocq-bst), [rbt-rocq](https://github.com/alpaylan/etna-rocq-rbt), [stlc-rocq](https://github.com/alpaylan/etna-rocq-stlc), [ifc-rocq](https://github.com/alpaylan/etna-rocq-ifc), [bst-proplang-rocq](https://github.com/alpaylan/etna-rocq-bst-proplang), [rbt-proplang-rocq](https://github.com/alpaylan/etna-rocq-rbt-proplang), [stlc-proplang-rocq](https://github.com/alpaylan/etna-rocq-stlc-proplang), [ifc-proplang-rocq](https://github.com/alpaylan/etna-rocq-ifc-proplang), [sorting-proplang-rocq](https://github.com/alpaylan/etna-rocq-sorting-proplang) |
| Racket   | RackCheck                                  | [bst-racket](https://github.com/alpaylan/etna-racket-bst), [rbt-racket](https://github.com/alpaylan/etna-racket-rbt), [stlc-racket](https://github.com/alpaylan/etna-racket-stlc), [systemf-racket](https://github.com/alpaylan/etna-racket-systemf)                                                                                                                                                                                                                                                                                                                                                                        |
| Rust     | QuickCheck(fork)                           | [bst-rust](https://github.com/alpaylan/etna-rust-bst), [rbt-rust](https://github.com/alpaylan/etna-rust-rbt), [stlc-rust](https://github.com/alpaylan/etna-rust-stlc), [sudoku-rust (stub)](https://github.com/alpaylan/etna-rust-sudoku)                                                                                                                                                                                                                                                                                                                                                                                    |
| OCaml    | QCheck, Base_quickcheck(WIP), Crowbar(WIP) | [bst-ocaml](https://github.com/alpaylan/etna-ocaml-bst), [rbt-ocaml](https://github.com/alpaylan/etna-ocaml-rbt), [stlc-ocaml](https://github.com/alpaylan/etna-ocaml-stlc), [rare-ocaml](https://github.com/alpaylan/etna-ocaml-rare)                                                                                                                                                                                                                                                                                                                                                                                       |
| Python   | Hypothesis                                 | [bst-python](https://github.com/alpaylan/etna-python-bst)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |

Shared support libraries (consumed as git submodules by their dependents): [etna-haskell-lib](https://github.com/alpaylan/etna-haskell-lib), [etna-ocaml-util](https://github.com/alpaylan/etna-ocaml-util), [etna-rocq-lib](https://github.com/alpaylan/etna-rocq-lib), [etna-rs-utils](https://github.com/alpaylan/etna-rs-utils).

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

### Catalog static site

`etna workload site --out ./site/` fetches every catalog entry's `etna.toml`
(plus referenced patch files) and emits a deployable directory. Combined with
the webview-ui bundle (`cd etna-vscode/webview-ui && npm run build`) it
produces a standalone browser view of the whole workload catalog.

`.github/workflows/deploy-site.yml` publishes this to Cloudflare Pages on
every push to `main`. One-time setup:

1. Create a Cloudflare API token scoped to `Cloudflare Pages — Edit` for
   the target account.
2. Add two repo secrets: `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`.
3. First run auto-creates the `etna-workloads` Pages project; subsequent
   pushes redeploy to the production URL.

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