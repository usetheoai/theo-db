"""Rust detector — wraps cargo-udeps (D1) + crates.io registry lookup (D2).

T1.3 implementation: detect_dead_code via cargo-udeps subprocess.
T2.4 implementation: detect_symbol_fabrication via tree-sitter + crates.io.
Other methods still stubs (T3.1) — T4.3 ADR DEFER for mutation.
"""
from __future__ import annotations

import re
import subprocess
from pathlib import Path

from scripts import _registry
from scripts._shared import Finding, safe_parse_json, sanitize_symbol, to_rel_path
from scripts.check_symbol_fab import extract_imports_and_calls

from . import BaseDetector, _arch

_ARCH_TIMEOUT_SEC = 600
_LAYERFILE = "Layerfile.toml"

_RUST_MODULE_LOCAL_PREFIXES = ("crate::", "self::", "super::", "crate", "self", "super")

# Crates that ship WITH the toolchain and are therefore never published on crates.io. Looking them up
# there answers "not found", which the D2 rubric would read as symbol fabrication — so a file containing
# `use std::collections::HashMap` scored FAIL_HARD. Measured on theo-db 2026-07-23: 117/117 D2 findings
# were false positives of exactly this shape (mostly `std`, plus `core` and same-crate modules).
_RUST_BUILTIN_CRATES = frozenset(
    {"std", "core", "alloc", "proc_macro", "test", "Self", "_"}
)


def _local_module_names(src_text: str) -> set[str]:
    """Names declared as modules IN THIS FILE (`mod foo;` / `mod foo {` / `pub mod foo`).

    A `use foo::Bar` whose first segment is one of these resolves inside the crate, not on crates.io —
    the same reason `crate::`/`self::`/`super::` are skipped. Without this, an `examples/` binary that
    `#[path]`-includes a module (the project's convention for testing PG-free logic) is reported as
    importing a fabricated crate.
    """
    return set(re.findall(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)", src_text, re.M))


def _workspace_crate_names(start: Path) -> set[str]:
    """`[package] name` of every Cargo.toml at or above `start` and in its workspace siblings.

    A path/workspace dependency (e.g. `theodb_lexical`) is a crate of THIS repo and is not published on
    crates.io — looking it up there answers "not found", which the rubric would read as fabrication.
    Walks up to the workspace root, then scans its subtree (bounded to Cargo.toml files, skipping
    `target/`), so members declared by glob are covered without parsing the manifest's TOML.
    """
    names: set[str] = set()
    root: Path | None = None
    for parent in [start, *start.parents]:
        manifest = parent / "Cargo.toml"
        if manifest.is_file():
            root = parent
            try:
                if "[workspace]" in manifest.read_text(encoding="utf-8", errors="replace"):
                    break
            except OSError:
                break
    if root is None:
        return names
    for manifest in root.rglob("Cargo.toml"):
        if "target" in manifest.parts:
            continue
        try:
            text = manifest.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        m = re.search(r'^\s*name\s*=\s*"([^"]+)"', text, re.M)
        if m:
            names.add(m.group(1).replace("-", "_"))
            names.add(m.group(1))
    return names


def _in_scope_names(src_text: str) -> set[str]:
    """Last segments of the other `use` statements in this file — names already brought into scope.

    `use pgrx::pg_sys;` followed by `use pg_sys::XactEvent as XE;` is a re-scoped module path, not a
    second crate. Treating the first segment as a crate name reports `pg_sys` as fabricated.
    """
    names: set[str] = set()
    for stmt in re.findall(r"^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+([^;{]+)", src_text, re.M):
        tail = stmt.strip().rstrip(":").split("::")[-1].strip()
        tail = tail.split(" as ")[0].strip()
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", tail):
            names.add(tail)
    return names


def _has_glob_import(src_text: str) -> bool:
    """Does this file contain a glob import (`use x::*;`)?

    A glob brings names into scope that no static scan can enumerate — `use pgrx::prelude::*;` is what
    makes `use pg_sys::XactEvent as XE;` legal three lines later. In such a file the detector CANNOT
    prove a first segment is a crate rather than a glob-imported module, so the honest severity is
    SOFT_FLOOR ("could not verify"), never HARD ("fabricated"). Claiming fabrication here would be the
    detector asserting something it did not establish.
    """
    return re.search(r"^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+[^;]*::\*\s*;", src_text, re.M) is not None


def _normalize_use_module(module: str) -> str:
    """Strip visibility/keyword noise the extractor can leave on a re-export (`pub use cache::X`)."""
    m = module.strip()
    for prefix in ("pub(crate) use ", "pub(super) use ", "pub use ", "use "):
        if m.startswith(prefix):
            m = m[len(prefix) :].strip()
    return m

_CARGO_UDEPS_TIMEOUT_SEC = 180


class RustDetector(BaseDetector):
    language = "rust"
    manifest_marker = "Cargo.toml"

    def detect_dead_code(self, manifest_dir: Path) -> list[Finding]:
        """Run `cargo +nightly udeps --output json` and parse unused deps."""
        cmd = ["cargo", "+nightly", "udeps", "--output", "json", "--all-targets"]
        try:
            result = subprocess.run(
                cmd,
                cwd=str(manifest_dir),
                capture_output=True,
                text=True,
                timeout=_CARGO_UDEPS_TIMEOUT_SEC,
                check=False,
            )
        except FileNotFoundError:
            return [self._auditor_unavailable("cargo +nightly udeps not found (install nightly + cargo-udeps)")]
        except subprocess.TimeoutExpired:
            return [self._auditor_unavailable(f"cargo-udeps timed out after {_CARGO_UDEPS_TIMEOUT_SEC}s")]
        except (subprocess.SubprocessError, OSError) as e:
            return [self._auditor_unavailable(f"cargo-udeps invocation failed: {e}")]

        if not result.stdout.strip():
            # No JSON output — likely no Cargo.toml or build error
            if result.returncode != 0:
                return [
                    self._auditor_unavailable(
                        f"cargo-udeps exit {result.returncode}: {result.stderr.strip()[:200]}"
                    )
                ]
            return []

        data, parse_finding = safe_parse_json(result.stdout, "cargo-udeps")
        if parse_finding is not None:
            return [
                Finding(
                    detector="d1_dead_code",
                    language="rust",
                    severity="SOFT_CAP",
                    file_path=".",
                    symbol_or_line="cargo-udeps",
                    message=f"cargo-udeps JSON output failed to parse: {parse_finding.message}",
                    allowlist_key="rust|.|dead_code|auditor_output_malformed_cargo-udeps",
                )
            ]
        return self._parse_udeps_json(data)

    def detect_symbol_fabrication(self, changed_files: list[Path]) -> list[Finding]:
        """T2.4 — Validate `use` statements against crates.io. Skip module-local (EC-17 analog)."""
        findings: list[Finding] = []
        # Vacuity guard (M146, #175). `extract_imports_and_calls` degrades to an empty list when the
        # tree-sitter grammar is missing or fails to load, and it says so to nobody. D2 then reports
        # "no findings", which the rubric reads as CLEAN — a silent false-green. Measured: on the build
        # droplet the extractor yielded 0 symbols for a 1600-line file that has 10 `use` statements,
        # and the audit still emitted PASS. A Rust file containing a `use` line MUST yield at least one
        # import when the parser works, so "some file had a `use`, yet nothing was extracted anywhere"
        # is proof the auditor did not run — reported as unavailable, never as clean.
        saw_use_line = False
        extracted_any = False
        for src_file in changed_files:
            if not src_file.exists():
                continue
            rel = to_rel_path(src_file)
            try:
                src_text = src_file.read_text(encoding="utf-8", errors="replace")
            except OSError:
                src_text = ""
            if re.search(r"^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+\S", src_text, re.M):
                saw_use_line = True
            local_mods = _local_module_names(src_text) | _in_scope_names(src_text)
            local_mods |= _workspace_crate_names(src_file.parent)
            undecidable = _has_glob_import(src_text)
            for sym in extract_imports_and_calls(src_file, "rust"):
                extracted_any = True
                if sym.kind != "import":
                    continue
                module = _normalize_use_module(sym.module or "")
                if not module:
                    continue
                # EC-17 analog — module-local
                if any(module == p or module.startswith(p) for p in _RUST_MODULE_LOCAL_PREFIXES):
                    continue
                # Extract crate name (first segment of "serde::Deserialize")
                crate = module.split("::", 1)[0].strip()
                if not crate:
                    continue
                # Toolchain crates are not on crates.io, and a module declared in THIS file resolves
                # inside the crate — neither is a fabricated symbol.
                if crate in _RUST_BUILTIN_CRATES or crate in local_mods:
                    continue
                exists = _registry.crate_exists_on_crates_io(crate)
                if exists is True:
                    continue
                sanitized = sanitize_symbol(crate)
                if exists is False and undecidable:
                    findings.append(
                        Finding(
                            detector="d2_symbol_fab",
                            language="rust",
                            severity="SOFT_FLOOR",
                            file_path=rel,
                            symbol_or_line=f"use {module}",
                            message=(
                                f"Could not verify '{crate}': not on crates.io, but this file has a "
                                "glob import, so it may be a glob-imported module"
                            ),
                            allowlist_key=f"rust|{rel}|symbol_fab|symbol_fab_unverifiable_{sanitized}",
                        )
                    )
                elif exists is False:
                    findings.append(
                        Finding(
                            detector="d2_symbol_fab",
                            language="rust",
                            severity="HARD",
                            file_path=rel,
                            symbol_or_line=f"use {module}",
                            message=f"Fabricated crate '{crate}' (not found on crates.io)",
                            allowlist_key=f"rust|{rel}|symbol_fab|{sanitized}",
                        )
                    )
                else:
                    findings.append(
                        Finding(
                            detector="d2_symbol_fab",
                            language="rust",
                            severity="SOFT_FLOOR",
                            file_path=rel,
                            symbol_or_line=f"use {module}",
                            message=f"Could not verify crate '{crate}' (ambiguous response)",
                            allowlist_key=f"rust|{rel}|symbol_fab|symbol_fab_unverifiable_{sanitized}",
                        )
                    )
        if saw_use_line and not extracted_any:
            return [
                Finding(
                    detector="d2_symbol_fab",
                    language="rust",
                    severity="SOFT_CAP",
                    file_path=".",
                    symbol_or_line="tree-sitter",
                    message=(
                        "D2 extracted 0 symbols from Rust sources that DO contain `use` statements — "
                        "the tree-sitter Rust grammar is unavailable or failed to load. The audit did "
                        "not run; this is NOT evidence that no symbol is fabricated."
                    ),
                    allowlist_key="rust|.|symbol_fab|auditor_unavailable_tree-sitter-rust",
                )
            ]
        return findings

    def detect_orphan_exports(self, repo_root: Path) -> list[Finding]:
        raise NotImplementedError("T3.1: cross-package wiring detector not yet implemented")

    def detect_mutation_score(self, critical_paths: list[Path]) -> list[Finding]:
        # T4.3 — DEFERRED to v0.2 (evaluate cargo-mutants vs gremlins first)
        raise NotImplementedError("T4.3: Rust mutation testing DEFERRED to v0.2 (graceful skip)")

    # ------------------------------------------------------------------

    def _parse_udeps_json(self, data: dict) -> list[Finding]:
        findings: list[Finding] = []
        unused = data.get("unused_deps", {}) or {}
        for crate_id, sections in unused.items():
            for section_name in ("normal", "development", "build"):
                for dep_name in sections.get(section_name, []) or []:
                    sanitized = sanitize_symbol(dep_name)
                    findings.append(
                        Finding(
                            detector="d1_dead_code",
                            language="rust",
                            severity="HARD",
                            file_path="Cargo.toml",
                            symbol_or_line=f"{dep_name} ({section_name}, crate={crate_id})",
                            message=f"Unused {section_name} dependency '{dep_name}' "
                            f"in crate {crate_id}",
                            allowlist_key=f"rust|Cargo.toml|dead_code|{sanitized}",
                        )
                    )
        return findings

    def _auditor_unavailable(self, reason: str) -> Finding:
        return Finding(
            detector="d1_dead_code",
            language="rust",
            severity="SOFT_CAP",
            file_path=".",
            symbol_or_line="cargo-udeps",
            message=f"cargo-udeps auditor unavailable: {reason}",
            allowlist_key="rust|.|dead_code|auditor_unavailable_cargo-udeps",
        )

    # ── D5 — architecture ───────────────────────────────────────────────────────────────────────

    def detect_architecture_violations(self, manifest_dir: Path) -> list[Finding]:
        """Run `layered-crate` against the crate's own `Layerfile.toml`.

        ## Why the Rust answer is thinner than the Go and TypeScript ones

        Rust has no maintained ArchUnit. Measured 2026-08-06: `cargo-archtest` and
        `arch_test_core`, the two direct equivalents, were both last published on 2021-06-15.
        `layered-crate` is alive (0.4.6, 2026-07-12, MIT) and purpose-built, so it is what D5
        drives — but it verifies by COMPILING, splitting the crate into one temporary package per
        layer and running `cargo check` on each. That makes it strictly stronger than an import
        scan and strictly more fragile.

        Measured against `theo-db`, the only Rust repo in the routing table, it did not run. Three
        blockers, in the order they appeared:

        1. `[lib]` declares `crate-type` but no `path` -> `failed to read lib.path from Cargo.toml`
        2. the temporary package it generates collides inside the crate's own cargo workspace
           (`two packages named theodb_rs`), and `workspace.exclude` did not clear it
        3. never reached: the crate is a pgrx extension and cannot compile without Postgres 18.4
           initialised, so per-layer `cargo check` needs the full extension toolchain

        The first two are changes to a database's build configuration made to accommodate a
        linter. That trade is the repo owner's to make, not Squad's — so D5 reports the blocker
        with its measurement and does not fail the cycle over it.

        ## Why the verdict is coarse

        `layered-crate` emits human-readable text, not JSON. D5 cannot reliably separate a real
        layer violation from a setup failure, so it classifies on known setup markers and says
        which one it decided. Guessing silently would be worse than a coarse answer that admits
        its own resolution.
        """
        config = manifest_dir / _LAYERFILE
        if not config.is_file():
            return [_arch.no_config("rust", tool="layered-crate", looked_for=[_LAYERFILE])]

        try:
            result = subprocess.run(
                ["layered-crate"],
                cwd=str(manifest_dir),
                capture_output=True,
                text=True,
                timeout=_ARCH_TIMEOUT_SEC,
                check=False,
            )
        except FileNotFoundError:
            return [
                _arch.auditor_unavailable(
                    "rust",
                    tool="layered-crate",
                    reason="binary not found (install via `cargo install layered-crate`)",
                )
            ]
        except subprocess.TimeoutExpired:
            return [
                _arch.auditor_unavailable(
                    "rust", tool="layered-crate", reason=f"timed out after {_ARCH_TIMEOUT_SEC}s"
                )
            ]
        except (subprocess.SubprocessError, OSError) as e:
            return [
                _arch.auditor_unavailable("rust", tool="layered-crate", reason=f"invocation failed: {e}")
            ]

        if result.returncode == 0:
            return []

        output = f"{result.stdout}\n{result.stderr}"
        blocker = _setup_blocker(output)
        if blocker is not None:
            return [_arch.auditor_unavailable("rust", tool="layered-crate", reason=blocker)]

        return [
            _arch.violation(
                "rust",
                tool="layered-crate",
                rule="layer-dependencies",
                file_path=_LAYERFILE,
                symbol_or_line="layers",
                message=(
                    "a layer imported one it does not declare a dependency on: "
                    f"{_tail(output)}"
                ),
            )
        ]


#: Failures that mean layered-crate could not START, not that a layer boundary was crossed. Each
#: string was observed against theo-db on 2026-08-06.
_SETUP_MARKERS = {
    "failed to read lib.path": (
        "the crate's `[lib]` declares no `path`. layered-crate requires it; adding "
        '`path = "src/lib.rs"` is the fix, and it is a change to the crate\'s build config'
    ),
    "two packages named": (
        "the temporary package layered-crate generates collides with the crate inside its own "
        "cargo workspace. Observed on theo-db even with the default temp dir under `target/`"
    ),
    "not found archfile": "no Layerfile.toml where layered-crate looked",
    ".pgrx/config.toml": (
        "the crate needs a pgrx-initialised Postgres to compile, and layered-crate verifies by "
        "compiling. Running it here requires `cargo pgrx init`, which builds Postgres from source"
    ),
}


def _setup_blocker(output: str) -> str | None:
    for marker, explanation in _SETUP_MARKERS.items():
        if marker in output:
            return explanation
    return None


def _tail(output: str, limit: int = 300) -> str:
    lines = [ln.strip() for ln in output.splitlines() if ln.strip()]
    return " / ".join(lines[-3:])[:limit]
