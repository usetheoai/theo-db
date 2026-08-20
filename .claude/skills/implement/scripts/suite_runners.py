#!/usr/bin/env python3
"""Language-aware test-suite runners for the /implement validation gate.

WHY this module exists
----------------------
`run_validation.py` shipped four executive checks — `npm test`, `npm run
typecheck`, `npm run lint`, `npm run test:coverage` — and every one of them
answered `SKIP` when `package.json` was absent. On a Python, Go or Rust repo the
whole executive half of the gate went quiet, `overall` became `PARTIAL`, and
`PARTIAL` exits 0. Since `rules/cycle-implement.md` says the completion promise
is emitted "EXCLUSIVELY when run_validation.py exits 0", the promise could be
emitted with no test having run at all.

The kit itself is multi-language (`rules/code-quality-languages.txt` enables
Python, Go, Rust and TypeScript) and this repository is Python — so the defect
fired hardest exactly where the kit dogfoods itself.

The fix has two halves, and both live here:

  1. Real runners for the other three languages, so the tests actually execute.
  2. `test_execution`, a consolidating gate that FAILs when a language manifest
     is present and *nothing* ran. A `SKIP` there is indistinguishable in the
     report from "legitimately nothing to check" — the same defect shape the
     `_find_progress` docstring in `run_validation.py` already named.

Honest scope: these runners assert that a suite executed and was green. They do
NOT assert the suite was meaningful — an empty-but-green `go test ./...` still
passes here. That question belongs to `check_test_obligations.py` and to
`/review`.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path
from typing import Any

#: Manifest files that prove a language is present at the repository root.
#: Order is stable so `languages_detected` reads the same way in every report.
LANGUAGE_MANIFESTS: dict[str, tuple[str, ...]] = {
    "javascript": ("package.json",),
    "python": ("pyproject.toml", "setup.py", "setup.cfg"),
    "go": ("go.mod", "go.work"),
    "rust": ("Cargo.toml",),
}


def run_command(cmd: list[str], cwd: Path, timeout: int = 300) -> dict[str, Any]:
    """Run a command, never raise. Shared by every check in the gate."""
    try:
        result = subprocess.run(
            cmd,
            cwd=str(cwd),
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return {
            "exit_code": result.returncode,
            "stdout_tail": result.stdout[-500:] if result.stdout else "",
            "stderr_tail": result.stderr[-500:] if result.stderr else "",
        }
    except subprocess.TimeoutExpired:
        return {"exit_code": -1, "error": f"timeout after {timeout}s"}
    except FileNotFoundError as exc:
        return {"exit_code": -1, "error": f"command not found: {exc}"}


def detect_languages(project_root: Path) -> list[str]:
    """Languages whose manifest sits at the repository root."""
    return [
        language
        for language, manifests in LANGUAGE_MANIFESTS.items()
        if any((project_root / manifest).exists() for manifest in manifests)
    ]


def _unavailable(text: str) -> bool:
    """The runner itself is missing, as opposed to the suite being red."""
    markers = ("No module named", "command not found", "not found")
    return any(marker in text for marker in markers)


def check_python_tests(project_root: Path) -> dict[str, Any]:
    """pytest, falling back to unittest discovery.

    A `pytest` exit code of 5 means "collected nothing". That is a FAIL here: a
    Python manifest promised a suite and none was found, which is the exact
    silence this gate exists to break.
    """
    name = "python tests"
    if "python" not in detect_languages(project_root):
        return {"name": name, "status": "SKIP", "reason": "no Python manifest at the repo root"}

    result = run_command(
        [sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider"],
        project_root,
        timeout=900,
    )
    code = result.get("exit_code")
    output = f"{result.get('stdout_tail', '')}{result.get('stderr_tail', '')}{result.get('error', '')}"

    if code == 0:
        return {"name": name, "status": "PASS", "runner": "pytest"}
    if code == 5:
        return {
            "name": name,
            "status": "FAIL",
            "runner": "pytest",
            "code": "no_tests_collected",
            "reason": "pytest collected no tests — a Python manifest with no suite is not a pass",
        }
    if not _unavailable(output):
        return {
            "name": name,
            "status": "FAIL",
            "runner": "pytest",
            "exit_code": code,
            "stderr_tail": result.get("stderr_tail", result.get("error", "")),
        }

    # pytest is not installed — try the stdlib runner before giving up.
    fallback = run_command(
        [sys.executable, "-m", "unittest", "discover", "-q"], project_root, timeout=900
    )
    fallback_output = f"{fallback.get('stdout_tail', '')}{fallback.get('stderr_tail', '')}"
    if fallback.get("exit_code") == 0 and "Ran 0 tests" not in fallback_output:
        return {"name": name, "status": "PASS", "runner": "unittest"}
    return {
        "name": name,
        "status": "FAIL",
        "runner": "unittest",
        "code": "runner_unavailable" if _unavailable(output) and "Ran 0 tests" not in fallback_output else "no_tests_collected",
        "reason": "neither pytest nor unittest discovery executed a test",
        "stderr_tail": fallback.get("stderr_tail", fallback.get("error", "")),
    }


_GO_USE_BLOCK_RE = re.compile(r"^use\s*\((.*?)^\)", re.MULTILINE | re.DOTALL)
_GO_USE_SINGLE_RE = re.compile(r"^use\s+(\S+)\s*$", re.MULTILINE)


def go_workspace_modules(project_root: Path) -> list[str]:
    """Modules a `go.work` lists, relative to the repo and inside it.

    `go test ./...` at a workspace root fails with "directory prefix . does not
    contain modules listed in go.work" — the kit already hit this shape in
    /arch-check. Paths that leave the repo (`../theo-contracts`) belong to a
    sibling repository with its own gates and are dropped, not audited from here.
    """
    work = project_root / "go.work"
    if not work.is_file():
        return []
    text = work.read_text(encoding="utf-8")
    raw: list[str] = []
    for block in _GO_USE_BLOCK_RE.findall(text):
        raw.extend(line.strip() for line in block.splitlines() if line.strip())
    raw.extend(_GO_USE_SINGLE_RE.findall(text))

    modules: list[str] = []
    for entry in raw:
        entry = entry.strip().strip('"')
        if not entry or entry.startswith(".."):
            continue
        rel = entry[2:] if entry.startswith("./") else entry
        if rel and (project_root / rel).is_dir() and rel not in modules:
            modules.append(rel)
    return modules


def check_go_tests(project_root: Path) -> dict[str, Any]:
    name = "go tests"
    if "go" not in detect_languages(project_root):
        return {"name": name, "status": "SKIP", "reason": "no go.mod at the repo root"}
    # A workspace root is not a module: run each module the go.work lists.
    modules = go_workspace_modules(project_root) if not (project_root / "go.mod").is_file() else []
    if modules:
        failures = []
        for module in modules:
            outcome = run_command(["go", "test", "./..."], project_root / module, timeout=900)
            text = f"{outcome.get('stderr_tail', '')}{outcome.get('error', '')}"
            if outcome.get("exit_code") == 0:
                continue
            if _unavailable(text):
                return {
                    "name": name, "status": "FAIL", "runner": "go test",
                    "code": "toolchain_unavailable",
                    "reason": "go.work present but the go toolchain is unavailable — "
                              "unverified is not verified",
                }
            failures.append({"module": module, "stderr_tail": outcome.get("stderr_tail", "")})
        if failures:
            return {"name": name, "status": "FAIL", "runner": "go test",
                    "modules_tested": modules, "failed_modules": [f["module"] for f in failures],
                    "stderr_tail": failures[0]["stderr_tail"]}
        return {"name": name, "status": "PASS", "runner": "go test", "modules_tested": modules}

    result = run_command(["go", "test", "./..."], project_root, timeout=900)
    output = f"{result.get('stderr_tail', '')}{result.get('error', '')}"
    if result.get("exit_code") == 0:
        return {"name": name, "status": "PASS", "runner": "go test"}
    if _unavailable(output):
        return {
            "name": name,
            "status": "FAIL",
            "runner": "go test",
            "code": "toolchain_unavailable",
            "reason": "go.mod present but the go toolchain is unavailable — unverified is not verified",
        }
    return {
        "name": name,
        "status": "FAIL",
        "runner": "go test",
        "exit_code": result.get("exit_code"),
        "stderr_tail": result.get("stderr_tail", result.get("error", "")),
    }


def check_rust_tests(project_root: Path) -> dict[str, Any]:
    name = "rust tests"
    if "rust" not in detect_languages(project_root):
        return {"name": name, "status": "SKIP", "reason": "no Cargo.toml at the repo root"}
    result = run_command(["cargo", "test", "--quiet"], project_root, timeout=1200)
    output = f"{result.get('stderr_tail', '')}{result.get('error', '')}"
    if result.get("exit_code") == 0:
        return {"name": name, "status": "PASS", "runner": "cargo test"}
    if _unavailable(output):
        return {
            "name": name,
            "status": "FAIL",
            "runner": "cargo test",
            "code": "toolchain_unavailable",
            "reason": "Cargo.toml present but the cargo toolchain is unavailable — unverified is not verified",
        }
    return {
        "name": name,
        "status": "FAIL",
        "runner": "cargo test",
        "exit_code": result.get("exit_code"),
        "stderr_tail": result.get("stderr_tail", result.get("error", "")),
    }


def check_test_execution(project_root: Path, suite_checks: list[dict[str, Any]]) -> dict[str, Any]:
    """Did ANY test suite actually execute?

    - No language manifest at all → SKIP. Pre-code phase is a legitimate nothing.
    - A manifest exists and at least one suite ran (green or red) → PASS. A red
      suite is already blocking through its own check; this gate only asks
      whether the question was put to a runner.
    - A manifest exists and nothing ran → FAIL. This is the case that used to
      exit 0.
    """
    languages = detect_languages(project_root)
    # A runner that started and found nothing, or that was not installed at all,
    # did not put the question to a suite — it only proved it could not.
    non_execution = {"no_tests_collected", "runner_unavailable", "toolchain_unavailable"}
    executed = [
        c for c in suite_checks
        if c.get("status") in ("PASS", "FAIL") and c.get("code") not in non_execution
    ]

    if not languages:
        return {
            "name": "test_execution",
            "status": "SKIP",
            "languages_detected": [],
            "reason": "no language manifest at the repo root — pre-code phase",
        }
    if executed:
        return {
            "name": "test_execution",
            "status": "PASS",
            "languages_detected": languages,
            "suites_executed": [c.get("name") for c in executed],
        }
    return {
        "name": "test_execution",
        "status": "FAIL",
        "languages_detected": languages,
        "suites_executed": [],
        "reason": (
            f"manifest(s) for {', '.join(languages)} present but no test suite executed. "
            "A gate that reports SKIP here is indistinguishable from one that verified "
            "something — so it fails instead."
        ),
        "skipped_reasons": [
            {"name": c.get("name"), "reason": c.get("reason")}
            for c in suite_checks
            if c.get("status") == "SKIP"
        ],
    }
