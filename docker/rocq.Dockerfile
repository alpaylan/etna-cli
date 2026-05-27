# Prebuilt Rocq toolchain image for `run-experiment.yml`'s `rocq-image` input.
#
# It bakes exactly what the from-source path in run-experiment.yml installs, so
# experiment CI can run inside this container and skip the ~30-min build:
#   * an AFL-instrumented OCaml 5.2.1 switch (the fuzzing workloads read AFL
#     coverage via shared memory from the compiled OCaml at runtime),
#   * Coq (Rocq 9.x) + QuickChick (alpaylan/QuickChick#programmable-pbt) + the
#     deps that branch's opam file declares (dune, mathcomp-ssreflect,
#     simple-io, ext-lib, ...).
#
# etna itself is NOT baked in: run-experiment.yml installs it fresh every run
# (to pick up the latest release) and puts it on PATH, so a copy here would only
# go stale and tie the image to etna's release arch matrix.
#
# Built/pushed by .github/workflows/build-rocq-image.yml to
# ghcr.io/<owner>/etna-quickchick:<tag>. Rebuild it whenever the QuickChick
# branch or the OCaml/Coq pins below change.
#
# Built on plain ubuntu (not coqorg/coq) because the AFL OCaml variant means
# Coq has to be compiled against that switch anyway, and running as root keeps
# GitHub Actions `container:` jobs — which mount the workspace as root — free of
# the permission dance stock Coq images need.
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# opam + the system bits Coq/QuickChick/dune build against, plus the tools the
# experiment job needs at runtime inside this container: git (checkout +
# QuickChick clone), curl/ca-certificates (run-experiment.yml's per-run etna
# install), sudo (some actions expect it), rsync/unzip (actions/cache +
# upload-artifact).
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git sudo \
        build-essential m4 pkg-config \
        libgmp-dev \
        bubblewrap rsync unzip \
        opam \
    && rm -rf /var/lib/apt/lists/*

ENV OPAMROOT=/root/.opam
ENV OPAMYES=1
ENV OPAMCONFIRMLEVEL=unsafe-yes

# `--disable-sandboxing` because bubblewrap can't get the namespaces it needs
# inside an unprivileged `docker build`.
RUN opam init --bare --disable-sandboxing --yes \
 && opam repository add coq-released https://coq.inria.fr/opam/released --dont-select --yes

# AFL switch — mirrors setup-ocaml's `ocaml-variants.5.2.1+options,ocaml-option-afl`.
RUN opam switch create etna ocaml-variants.5.2.1+options ocaml-option-afl \
        --repos=default,coq-released --yes

# Bake the switch into the image env. GitHub Actions runs each `run:` step in a
# fresh non-login shell that never sources `opam env`, so coqc/dune/quickchick
# must be discoverable from the image ENV alone. These are the standard opam
# switch paths for OPAMROOT=/root/.opam, switch `etna`.
ENV OPAMSWITCH=etna
ENV OPAM_SWITCH_PREFIX=/root/.opam/etna
ENV PATH=/root/.opam/etna/bin:$PATH
ENV CAML_LD_LIBRARY_PATH=/root/.opam/etna/lib/stublibs:/root/.opam/etna/lib/ocaml/stublibs
ENV OCAML_TOPLEVEL_PATH=/root/.opam/etna/lib/toplevel
ENV OCAMLPATH=/root/.opam/etna/lib

# QuickChick straight from the clone: opam reads its opam file, pulls the deps
# it declares, then builds and installs into the switch. No pins, no hard-coded
# versions — identical to run-experiment.yml's host path.
RUN git clone --depth 1 --branch programmable-pbt \
        https://github.com/alpaylan/QuickChick.git /tmp/quickchick-pl \
 && eval "$(opam env --switch=etna --set-switch)" \
 && opam install -y /tmp/quickchick-pl \
 && opam clean --all-switches --download-cache --logs --repo-cache \
 && rm -rf /tmp/quickchick-pl

# Fail the build early if the toolchain the experiment job depends on isn't on
# PATH from the image ENV alone (i.e. without `opam env`) — exactly the
# condition each GitHub Actions step shell runs under.
RUN coqc --version && dune --version
