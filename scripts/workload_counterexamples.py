#!/usr/bin/env python3
"""
Discover and validate workload counterexamples against mutated Rust workloads.

Examples:
  python3 scripts/workload_counterexamples.py discover --workload bst
  python3 scripts/workload_counterexamples.py validate --workload bst --workload rbt --workload stlc
"""

from __future__ import annotations

import argparse
import copy
import json
import os
import subprocess
import tempfile
from pathlib import Path
from typing import Any


SUPPORTED_WORKLOADS = ("bst", "rbt", "stlc")
Sexp = str | list["Sexp"]


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def normalize_status(status: str) -> str:
    return status.replace("_", "").replace("-", "").lower()


def run_command(
    cmd: list[str],
    *,
    cwd: Path | None = None,
    timeout: float | None = None,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(
        cmd,
        cwd=str(cwd) if cwd is not None else None,
        text=True,
        capture_output=True,
        timeout=timeout,
    )
    if check and proc.returncode != 0:
        command = " ".join(cmd)
        raise RuntimeError(
            f"Command failed ({proc.returncode}): {command}\n"
            f"stdout:\n{proc.stdout}\n"
            f"stderr:\n{proc.stderr}"
        )
    return proc


def parse_json_output(raw: str) -> dict[str, Any] | list[Any]:
    text = raw.strip()
    if not text:
        raise RuntimeError("Expected JSON output, got empty stdout")
    try:
        return json.loads(text)
    except json.JSONDecodeError as err:
        raise RuntimeError(f"Failed to parse JSON output:\n{text}") from err


def sexp_tokenize(text: str) -> list[str]:
    tokens: list[str] = []
    current = []
    for ch in text:
        if ch in ("(", ")"):
            if current:
                tokens.append("".join(current))
                current.clear()
            tokens.append(ch)
        elif ch.isspace():
            if current:
                tokens.append("".join(current))
                current.clear()
        else:
            current.append(ch)
    if current:
        tokens.append("".join(current))
    return tokens


def sexp_parse_tokens(tokens: list[str], i: int) -> tuple[Sexp, int]:
    if i >= len(tokens):
        raise RuntimeError("Unexpected end of token stream")

    tok = tokens[i]
    if tok == "(":
        i += 1
        items: list[Sexp] = []
        while i < len(tokens) and tokens[i] != ")":
            item, i = sexp_parse_tokens(tokens, i)
            items.append(item)
        if i >= len(tokens) or tokens[i] != ")":
            raise RuntimeError("Unbalanced parentheses in s-expression")
        return items, i + 1
    if tok == ")":
        raise RuntimeError("Unexpected ')' in s-expression")
    return tok, i + 1


def sexp_parse(text: str) -> Sexp:
    tokens = sexp_tokenize(text)
    node, i = sexp_parse_tokens(tokens, 0)
    if i != len(tokens):
        raise RuntimeError(f"Trailing tokens in s-expression: {tokens[i:]}")
    return node


def sexp_to_string(node: Sexp) -> str:
    if isinstance(node, str):
        return node
    return "(" + " ".join(sexp_to_string(item) for item in node) + ")"


def sexp_iter_paths(node: Sexp, path: tuple[int, ...] = ()) -> list[tuple[int, ...]]:
    paths = [path]
    if isinstance(node, list):
        for i, child in enumerate(node):
            paths.extend(sexp_iter_paths(child, path + (i,)))
    return paths


def sexp_get(node: Sexp, path: tuple[int, ...]) -> Sexp:
    cur = node
    for idx in path:
        if not isinstance(cur, list):
            raise RuntimeError(f"Invalid path {path} for s-expression node")
        cur = cur[idx]
    return cur


def sexp_replace(node: Sexp, path: tuple[int, ...], replacement: Sexp) -> Sexp:
    if not path:
        return copy.deepcopy(replacement)
    if not isinstance(node, list):
        raise RuntimeError(f"Invalid path {path} for non-list node")
    idx = path[0]
    out = [copy.deepcopy(item) for item in node]
    out[idx] = sexp_replace(out[idx], path[1:], replacement)
    return out


def sexp_size(node: Sexp) -> int:
    if isinstance(node, str):
        return 1
    return 1 + sum(sexp_size(child) for child in node)


def atom(node: Sexp) -> str | None:
    return node if isinstance(node, str) else None


def list_tag(node: Sexp, tag: str) -> bool:
    return isinstance(node, list) and bool(node) and atom(node[0]) == tag


def parse_int_atom(node: Sexp) -> int | None:
    if not isinstance(node, str):
        return None
    try:
        return int(node)
    except ValueError:
        return None


def is_tree_leaf(node: Sexp) -> bool:
    return atom(node) == "E" or (isinstance(node, list) and len(node) == 1 and atom(node[0]) == "E")


def unique_sexps(items: list[Sexp]) -> list[Sexp]:
    out: list[Sexp] = []
    seen = set()
    for item in items:
        s = sexp_to_string(item)
        if s in seen:
            continue
        seen.add(s)
        out.append(item)
    return out


def workload_dir(root: Path, workload: str) -> Path:
    return root / "workloads" / "Rust" / workload


def docs_path(root: Path, workload: str) -> Path:
    return root / "docs" / "workloads" / f"{workload}.json"


def serialized_bin(root: Path, workload: str) -> Path:
    return workload_dir(root, workload) / "target" / "release" / f"{workload}-serialized"


def sampler_bin(root: Path, workload: str) -> Path:
    return workload_dir(root, workload) / "target" / "release" / f"{workload}-sampler"


def quicktest_bin(root: Path, workload: str) -> Path:
    return workload_dir(root, workload) / "target" / "release" / workload


def reset_mutations(root: Path, workload: str) -> None:
    run_command(["marauders", "reset", "--path", str(workload_dir(root, workload))])


def set_mutations(root: Path, workload: str, mutations: list[str]) -> None:
    reset_mutations(root, workload)
    for variant in mutations:
        run_command(
            [
                "marauders",
                "set",
                "--path",
                str(workload_dir(root, workload)),
                "--variant",
                variant,
            ]
        )


def build_workload(root: Path, workload: str) -> None:
    run_command(
        [
            "cargo",
            "build",
            "--release",
            "--manifest-path",
            str(workload_dir(root, workload) / "Cargo.toml"),
        ],
        cwd=root,
    )


def run_serialized_counterexample(
    root: Path,
    workload: str,
    property_name: str,
    counterexample: str,
) -> dict[str, Any]:
    tests_expr = f"({counterexample})"
    with tempfile.NamedTemporaryFile("w", delete=False) as temp_file:
        temp_file.write(tests_expr)
        tests_file = temp_file.name
    try:
        proc = run_command(
            [str(serialized_bin(root, workload)), tests_file, property_name], check=False
        )
    finally:
        os.unlink(tests_file)
    result = parse_json_output(proc.stdout)
    if not isinstance(result, dict):
        raise RuntimeError(
            f"Serialized runner returned non-object JSON for {workload}/{property_name}: {result}"
        )
    return result


def discover_with_quicktest(
    root: Path, workload: str, property_name: str, timeout_seconds: float
) -> str | None:
    try:
        proc = run_command(
            [str(quicktest_bin(root, workload)), "quickcheck", property_name],
            timeout=timeout_seconds,
        )
    except subprocess.TimeoutExpired:
        return None

    payload = parse_json_output(proc.stdout)
    if not isinstance(payload, dict):
        return None

    if normalize_status(str(payload.get("status", ""))) != "failed":
        return None

    counterexample = str(payload.get("counterexample", "")).strip()
    if not counterexample:
        return None

    result = run_serialized_counterexample(root, workload, property_name, counterexample)
    if normalize_status(str(result.get("status", ""))) == "foundbug":
        return str(result.get("counterexample", "")).strip()
    return None


def discover_with_sampler(
    root: Path,
    workload: str,
    property_name: str,
    batch_size: int,
    max_batches: int,
) -> str | None:
    sampler = sampler_bin(root, workload)
    serialized = serialized_bin(root, workload)

    for _ in range(max_batches):
        sample_proc = run_command(
            [str(sampler), "quickcheck", property_name, str(batch_size)]
        )
        payload = parse_json_output(sample_proc.stdout)
        if not isinstance(payload, list):
            raise RuntimeError(f"Sampler returned non-array JSON: {payload}")

        values: list[str] = []
        for item in payload:
            if not isinstance(item, dict):
                raise RuntimeError(f"Malformed sampler item: {item}")
            value = item.get("value")
            if not isinstance(value, str):
                raise RuntimeError(f"Missing 'value' in sampler item: {item}")
            values.append(value)

        tests_expr = "(" + " ".join(values) + ")"
        with tempfile.NamedTemporaryFile("w", delete=False) as temp_file:
            temp_file.write(tests_expr)
            tests_file = temp_file.name
        try:
            run_proc = run_command(
                [str(serialized), tests_file, property_name],
                check=False,
            )
        finally:
            os.unlink(tests_file)

        result = parse_json_output(run_proc.stdout)
        if not isinstance(result, dict):
            raise RuntimeError(f"Serialized runner returned non-object JSON: {result}")

        status = normalize_status(str(result.get("status", "")))
        if status == "foundbug":
            return str(result.get("counterexample", "")).strip()
        if status != "finished":
            raise RuntimeError(
                f"Unexpected serialized status for {workload}/{property_name}: {result}"
            )
    return None


def discover_counterexample(
    root: Path,
    workload: str,
    property_name: str,
    *,
    quicktest_timeout: float,
    sampler_batch_size: int,
    sampler_max_batches: int,
) -> str:
    counterexample = discover_with_quicktest(
        root, workload, property_name, quicktest_timeout
    )
    if counterexample:
        return counterexample

    counterexample = discover_with_sampler(
        root,
        workload,
        property_name,
        batch_size=sampler_batch_size,
        max_batches=sampler_max_batches,
    )
    if counterexample:
        return counterexample

    raise RuntimeError(
        f"No counterexample found for workload={workload} property={property_name}"
    )


def replacement_candidates(node: Sexp, workload: str) -> list[Sexp]:
    candidates: list[Sexp] = []

    n = parse_int_atom(node)
    if n is not None:
        candidates.extend(["0", "1", "-1", "2", "-2"])
        candidates.append(str(n // 2))
        if n > 0:
            candidates.append(str(n - 1))
        if n < 0:
            candidates.append(str(n + 1))

    if atom(node) == "#f":
        candidates.append("#t")
    if atom(node) == "R":
        candidates.append("B")
    if isinstance(node, list) and len(node) == 1 and atom(node[0]) in {"E", "B", "R", "TBool"}:
        candidates.append(atom(node[0]))  # unwrap one-element constructor

    if workload in {"bst", "rbt"} and is_tree_leaf(node):
        # Normalize leaves to atom style when possible.
        candidates.append("E")
        candidates.append(["E"])

    if workload == "bst" and list_tag(node, "T") and isinstance(node, list) and len(node) == 5:
        _, left, key, value, right = node
        candidates.extend(
            [
                "E",
                left,
                right,
                ["T", "E", key, value, "E"],
                ["T", "E", "0", "0", "E"],
                ["T", left, "0", value, right],
                ["T", left, key, "0", right],
                ["T", "E", key, value, right],
                ["T", left, key, value, "E"],
            ]
        )

    if workload == "rbt" and list_tag(node, "T") and isinstance(node, list) and len(node) == 6:
        _, color, left, key, value, right = node
        candidates.extend(
            [
                "E",
                left,
                right,
                ["T", "B", "E", "0", "0", "E"],
                ["T", color, "E", key, value, "E"],
                ["T", "B", left, key, value, right],
                ["T", color, left, "0", value, right],
                ["T", color, left, key, "0", right],
            ]
        )

    if workload == "stlc":
        if list_tag(node, "Var") and isinstance(node, list) and len(node) == 2:
            candidates.append(["Var", "0"])
        elif list_tag(node, "Bool") and isinstance(node, list) and len(node) == 2:
            candidates.append(["Bool", "#t"])
        elif list_tag(node, "Abs") and isinstance(node, list) and len(node) == 3:
            _, typ, body = node
            candidates.extend(
                [
                    body,
                    ["Abs", typ, ["Var", "0"]],
                    ["Abs", "TBool", body],
                    ["Abs", "TBool", ["Var", "0"]],
                    ["Abs", "TBool", ["Bool", "#t"]],
                ]
            )
        elif list_tag(node, "App") and isinstance(node, list) and len(node) == 3:
            _, func, arg = node
            candidates.extend(
                [
                    func,
                    arg,
                    ["App", func, ["Bool", "#t"]],
                    ["App", ["Abs", "TBool", ["Var", "0"]], arg],
                    ["App", ["Abs", "TBool", ["Var", "0"]], ["Bool", "#t"]],
                ]
            )

        if list_tag(node, "TFun") and isinstance(node, list) and len(node) == 3:
            _, t1, t2 = node
            candidates.extend(["TBool", t1, t2, ["TFun", "TBool", "TBool"]])

    filtered: list[Sexp] = []
    for candidate in unique_sexps(candidates):
        if sexp_to_string(candidate) != sexp_to_string(node):
            filtered.append(candidate)
    return filtered


def minimize_counterexample(
    root: Path,
    workload: str,
    property_name: str,
    counterexample: str,
    *,
    max_calls: int,
    max_rounds: int,
) -> tuple[str, int]:
    cache: dict[str, bool] = {}

    def fails(expr: str) -> bool:
        if expr in cache:
            return cache[expr]
        result = run_serialized_counterexample(root, workload, property_name, expr)
        is_bug = normalize_status(str(result.get("status", ""))) == "foundbug"
        cache[expr] = is_bug
        return is_bug

    current_node = sexp_parse(counterexample)
    current = sexp_to_string(current_node)
    if not fails(current):
        raise RuntimeError(
            f"Given counterexample does not fail for {workload}/{property_name}: {counterexample}"
        )

    calls = 0
    for _ in range(max_rounds):
        improved = False
        paths = [p for p in sexp_iter_paths(current_node) if p]
        paths.sort(
            key=lambda p: len(sexp_to_string(sexp_get(current_node, p))),
            reverse=True,
        )

        for path in paths:
            node = sexp_get(current_node, path)
            candidates = replacement_candidates(node, workload)
            candidates.sort(key=lambda c: len(sexp_to_string(c)))

            for replacement in candidates:
                candidate_node = sexp_replace(current_node, path, replacement)
                candidate = sexp_to_string(candidate_node)
                if len(candidate) >= len(current):
                    continue
                calls += 1
                if calls > max_calls:
                    return current, calls
                if fails(candidate):
                    current_node = sexp_parse(candidate)
                    current = candidate
                    improved = True
                    break
            if improved:
                break
        if not improved:
            break
    return current, calls


def load_workload_docs(path: Path) -> list[dict[str, Any]]:
    payload = parse_json_output(path.read_text())
    if not isinstance(payload, list):
        raise RuntimeError(f"{path} must contain a JSON array")
    return payload


def write_workload_docs(path: Path, payload: list[dict[str, Any]]) -> None:
    path.write_text(json.dumps(payload, indent=4) + "\n")


def validate_workload(root: Path, workload: str, data: list[dict[str, Any]]) -> int:
    failures = 0
    try:
        for entry in data:
            mutations = entry.get("mutations")
            if not isinstance(mutations, list) or not all(
                isinstance(m, str) for m in mutations
            ):
                raise RuntimeError(
                    f"Invalid 'mutations' field in {docs_path(root, workload)}: {entry}"
                )
            print(f"[{workload}] mutations={mutations}")
            set_mutations(root, workload, mutations)
            build_workload(root, workload)

            tasks = entry.get("tasks")
            if not isinstance(tasks, list):
                raise RuntimeError(
                    f"Invalid 'tasks' field in {docs_path(root, workload)}: {entry}"
                )
            for task in tasks:
                property_name = task.get("property")
                counterexample = task.get("counterexample")
                if not isinstance(property_name, str) or not isinstance(
                    counterexample, str
                ):
                    raise RuntimeError(f"Invalid task entry: {task}")
                if not counterexample.strip():
                    print(f"  - {property_name}: FAIL (empty counterexample)")
                    failures += 1
                    continue

                result = run_serialized_counterexample(
                    root, workload, property_name, counterexample
                )
                status = normalize_status(str(result.get("status", "")))
                if status == "foundbug":
                    print(f"  - {property_name}: OK")
                else:
                    print(f"  - {property_name}: FAIL ({result})")
                    failures += 1
    finally:
        reset_mutations(root, workload)
    return failures


def discover_workload(
    root: Path,
    workload: str,
    data: list[dict[str, Any]],
    *,
    refresh: bool,
    quicktest_timeout: float,
    sampler_batch_size: int,
    sampler_max_batches: int,
) -> list[dict[str, Any]]:
    try:
        for entry in data:
            mutations = entry.get("mutations")
            if not isinstance(mutations, list) or not all(
                isinstance(m, str) for m in mutations
            ):
                raise RuntimeError(
                    f"Invalid 'mutations' field in {docs_path(root, workload)}: {entry}"
                )

            print(f"[{workload}] discovering for mutations={mutations}")
            set_mutations(root, workload, mutations)
            build_workload(root, workload)

            tasks = entry.get("tasks")
            if not isinstance(tasks, list):
                raise RuntimeError(
                    f"Invalid 'tasks' field in {docs_path(root, workload)}: {entry}"
                )

            for task in tasks:
                property_name = task.get("property")
                if not isinstance(property_name, str):
                    raise RuntimeError(f"Invalid task entry: {task}")

                current = task.get("counterexample", "")
                if not refresh and isinstance(current, str) and current.strip():
                    print(f"  - {property_name}: keep existing")
                    continue

                counterexample = discover_counterexample(
                    root,
                    workload,
                    property_name,
                    quicktest_timeout=quicktest_timeout,
                    sampler_batch_size=sampler_batch_size,
                    sampler_max_batches=sampler_max_batches,
                )
                task["counterexample"] = counterexample
                print(f"  - {property_name}: discovered")
    finally:
        reset_mutations(root, workload)
    return data


def minimize_workload(
    root: Path,
    workload: str,
    data: list[dict[str, Any]],
    *,
    max_calls_per_task: int,
    max_rounds: int,
) -> list[dict[str, Any]]:
    try:
        for entry in data:
            mutations = entry.get("mutations")
            if not isinstance(mutations, list) or not all(
                isinstance(m, str) for m in mutations
            ):
                raise RuntimeError(
                    f"Invalid 'mutations' field in {docs_path(root, workload)}: {entry}"
                )

            print(f"[{workload}] minimizing for mutations={mutations}")
            set_mutations(root, workload, mutations)
            build_workload(root, workload)

            tasks = entry.get("tasks")
            if not isinstance(tasks, list):
                raise RuntimeError(
                    f"Invalid 'tasks' field in {docs_path(root, workload)}: {entry}"
                )

            for task in tasks:
                property_name = task.get("property")
                counterexample = task.get("counterexample")
                if not isinstance(property_name, str) or not isinstance(
                    counterexample, str
                ):
                    raise RuntimeError(f"Invalid task entry: {task}")

                if not counterexample.strip():
                    print(f"  - {property_name}: skip (empty)")
                    continue

                minimized, calls = minimize_counterexample(
                    root,
                    workload,
                    property_name,
                    counterexample,
                    max_calls=max_calls_per_task,
                    max_rounds=max_rounds,
                )
                if len(minimized) < len(counterexample):
                    print(
                        f"  - {property_name}: {len(counterexample)} -> {len(minimized)} (calls={calls})"
                    )
                    task["counterexample"] = minimized
                else:
                    print(f"  - {property_name}: unchanged ({len(counterexample)})")
    finally:
        reset_mutations(root, workload)
    return data


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Discover/validate workload counterexamples for Rust workloads."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    common = argparse.ArgumentParser(add_help=False)
    common.add_argument(
        "--workload",
        action="append",
        choices=list(SUPPORTED_WORKLOADS),
        help=f"Workload to process (default: all of {SUPPORTED_WORKLOADS})",
    )

    validate_parser = subparsers.add_parser(
        "validate",
        parents=[common],
        help="Validate docs/workloads/<workload>.json counterexamples.",
    )
    validate_parser.set_defaults(command_name="validate")

    discover_parser = subparsers.add_parser(
        "discover",
        parents=[common],
        help="Discover and write missing counterexamples into docs/workloads/<workload>.json.",
    )
    discover_parser.add_argument(
        "--refresh",
        action="store_true",
        help="Recompute all counterexamples, not only missing ones.",
    )
    discover_parser.add_argument(
        "--quicktest-timeout",
        type=float,
        default=120.0,
        help="Timeout in seconds for quicktest attempts per property.",
    )
    discover_parser.add_argument(
        "--sampler-batch-size",
        type=int,
        default=2000,
        help="Number of samples per sampler batch fallback.",
    )
    discover_parser.add_argument(
        "--sampler-max-batches",
        type=int,
        default=200,
        help="Maximum number of sampler fallback batches per property.",
    )
    discover_parser.set_defaults(command_name="discover")

    minimize_parser = subparsers.add_parser(
        "minimize",
        parents=[common],
        help="Minimize existing counterexamples in docs/workloads/<workload>.json.",
    )
    minimize_parser.add_argument(
        "--max-calls-per-task",
        type=int,
        default=400,
        help="Maximum oracle checks while minimizing each property counterexample.",
    )
    minimize_parser.add_argument(
        "--max-rounds",
        type=int,
        default=80,
        help="Maximum greedy minimization rounds per counterexample.",
    )
    minimize_parser.set_defaults(command_name="minimize")

    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = repo_root()
    workloads = args.workload or list(SUPPORTED_WORKLOADS)

    if args.command_name == "validate":
        failures = 0
        for workload in workloads:
            path = docs_path(root, workload)
            payload = load_workload_docs(path)
            failures += validate_workload(root, workload, payload)
        if failures:
            print(f"\nValidation completed with {failures} failure(s).")
            return 1
        print("\nValidation completed successfully.")
        return 0

    if args.command_name == "discover":
        for workload in workloads:
            path = docs_path(root, workload)
            payload = load_workload_docs(path)
            updated = discover_workload(
                root,
                workload,
                payload,
                refresh=args.refresh,
                quicktest_timeout=args.quicktest_timeout,
                sampler_batch_size=args.sampler_batch_size,
                sampler_max_batches=args.sampler_max_batches,
            )
            write_workload_docs(path, updated)
            print(f"[{workload}] wrote {path}")
        return 0

    if args.command_name == "minimize":
        for workload in workloads:
            path = docs_path(root, workload)
            payload = load_workload_docs(path)
            updated = minimize_workload(
                root,
                workload,
                payload,
                max_calls_per_task=args.max_calls_per_task,
                max_rounds=args.max_rounds,
            )
            write_workload_docs(path, updated)
            print(f"[{workload}] wrote {path}")
        return 0

    raise RuntimeError(f"Unknown command: {args.command_name}")


if __name__ == "__main__":
    raise SystemExit(main())
