"""T1.3 — RustDetector.detect_dead_code (cargo-udeps wrapper) tests.

cargo-udeps requires nightly toolchain. Tests use subprocess mocks.
"""
from __future__ import annotations

import json
from pathlib import Path
from unittest.mock import patch

import pytest

from scripts.detectors.rust import RustDetector

pytestmark = pytest.mark.rust


_UDEPS_POSITIVE_JSON = json.dumps(
    {
        "success": False,
        "unused_deps": {
            "my-crate 0.1.0": {
                "manifest_path": "/abs/Cargo.toml",
                "normal": ["unused-crate"],
                "development": [],
                "build": [],
            }
        },
    }
)

_UDEPS_CLEAN_JSON = json.dumps({"success": True, "unused_deps": {}})


def _mock_run(stdout: str, returncode: int = 0, stderr: str = ""):
    class _R:
        def __init__(self) -> None:
            self.stdout = stdout
            self.stderr = stderr
            self.returncode = returncode

    return _R()


def test_rust_detector_flags_unused_dep(tmp_path: Path) -> None:
    det = RustDetector()
    with patch("subprocess.run", return_value=_mock_run(_UDEPS_POSITIVE_JSON, 1)):
        findings = det.detect_dead_code(tmp_path)
    dead = [f for f in findings if f.detector == "d1_dead_code"]
    assert any("unused-crate" in f.symbol_or_line for f in dead)


def test_rust_detector_no_findings_on_clean(tmp_path: Path) -> None:
    det = RustDetector()
    with patch("subprocess.run", return_value=_mock_run(_UDEPS_CLEAN_JSON, 0)):
        findings = det.detect_dead_code(tmp_path)
    dead = [f for f in findings if f.detector == "d1_dead_code"]
    assert dead == []


def test_rust_detector_emits_auditor_unavailable_when_nightly_missing(tmp_path: Path) -> None:
    det = RustDetector()
    with patch("subprocess.run", side_effect=FileNotFoundError("cargo +nightly missing")):
        findings = det.detect_dead_code(tmp_path)
    assert len(findings) == 1
    assert "auditor_unavailable_cargo-udeps" in findings[0].allowlist_key


def test_rust_detector_handles_malformed_json(tmp_path: Path) -> None:
    det = RustDetector()
    with patch("subprocess.run", return_value=_mock_run("not json at all", 1)):
        findings = det.detect_dead_code(tmp_path)
    assert len(findings) == 1
    assert "auditor_output_malformed_cargo-udeps" in findings[0].allowlist_key


# --------------------------------------------------------------------------
# D2 false-positive guards — ported with the detector, which shipped untested
# --------------------------------------------------------------------------

from scripts.detectors.rust import (  # noqa: E402
    _RUST_BUILTIN_CRATES,
    _has_glob_import,
    _in_scope_names,
    _local_module_names,
    _normalize_use_module,
    _workspace_crate_names,
)


class TestLocalModuleNames:
    """`mod foo;` in this file means `use foo::X` resolves in-crate, not on crates.io."""

    def test_finds_plain_bare_and_pub_and_scoped_declarations(self) -> None:
        src = (
            "mod plain;\n"
            "pub mod exported;\n"
            "pub(crate) mod scoped;\n"
            "mod inline { fn f() {} }\n"
        )
        assert _local_module_names(src) == {"plain", "exported", "scoped", "inline"}

    def test_ignores_the_word_mod_inside_an_identifier(self) -> None:
        assert _local_module_names("let module_count = 1;\nfn modify() {}\n") == set()

    def test_empty_source_declares_nothing(self) -> None:
        assert _local_module_names("") == set()


class TestBuiltinCrates:
    """The 117/117 false positives that motivated the fix were mostly `std`."""

    def test_toolchain_crates_are_listed(self) -> None:
        for crate in ("std", "core", "alloc", "proc_macro", "test"):
            assert crate in _RUST_BUILTIN_CRATES

    def test_a_real_published_crate_is_not_listed(self) -> None:
        assert "serde" not in _RUST_BUILTIN_CRATES
        assert "pgrx" not in _RUST_BUILTIN_CRATES


class TestInScopeNames:
    """`use pgrx::pg_sys;` makes a later `use pg_sys::X` a re-scoped path, not a crate."""

    def test_collects_the_last_segment_of_each_use(self) -> None:
        assert _in_scope_names("use pgrx::pg_sys;\nuse std::collections::HashMap;\n") == {
            "pg_sys",
            "HashMap",
        }

    def test_strips_an_alias(self) -> None:
        assert "XactEvent" in _in_scope_names("use pg_sys::XactEvent as XE;\n")

    def test_a_brace_group_yields_the_crate_segment_not_the_braced_names(self) -> None:
        """Known imprecision, pinned so a future change is a decision and not an accident.

        The capture stops at `{`, so `use foo::{A, B};` yields `foo` — the crate — rather
        than `A` and `B`, the names actually brought into scope. Consequence: a later
        `use foo::X` is skipped as already-in-scope and `foo` is never checked against
        crates.io. That under-reports, which is the safe direction for this detector: it
        declines to claim fabrication it did not establish.
        """
        assert _in_scope_names("use foo::{A, B};\n") == {"foo"}


class TestHasGlobImport:
    """A glob brings in names no static scan can enumerate — severity must soften."""

    def test_detects_a_glob(self) -> None:
        assert _has_glob_import("use pgrx::prelude::*;\n") is True

    def test_detects_a_pub_glob(self) -> None:
        assert _has_glob_import("pub use crate::api::*;\n") is True

    def test_plain_import_is_not_a_glob(self) -> None:
        assert _has_glob_import("use pgrx::prelude::PgRelation;\n") is False


class TestNormalizeUseModule:
    def test_strips_each_visibility_prefix(self) -> None:
        assert _normalize_use_module("pub use cache::X") == "cache::X"
        assert _normalize_use_module("pub(crate) use cache::X") == "cache::X"
        assert _normalize_use_module("pub(super) use cache::X") == "cache::X"
        assert _normalize_use_module("use cache::X") == "cache::X"

    def test_leaves_a_bare_path_untouched(self) -> None:
        assert _normalize_use_module("serde::Deserialize") == "serde::Deserialize"


class TestWorkspaceCrateNames:
    """A path/workspace dependency is ours; crates.io answers 'not found' for it."""

    def test_collects_member_names_and_normalises_hyphens(self, tmp_path) -> None:
        (tmp_path / "Cargo.toml").write_text('[workspace]\nmembers = ["a", "b"]\n')
        for name in ("a", "b"):
            member = tmp_path / name
            member.mkdir()
            (member / "Cargo.toml").write_text(f'[package]\nname = "theo-{name}"\n')

        names = _workspace_crate_names(tmp_path / "a")

        assert {"theo-a", "theo_a", "theo-b", "theo_b"} <= names

    def test_skips_the_target_directory(self, tmp_path) -> None:
        (tmp_path / "Cargo.toml").write_text('[workspace]\n[package]\nname = "root"\n')
        vendored = tmp_path / "target" / "debug" / "vendored"
        vendored.mkdir(parents=True)
        (vendored / "Cargo.toml").write_text('[package]\nname = "not-ours"\n')

        assert "not-ours" not in _workspace_crate_names(tmp_path)

    def test_no_manifest_anywhere_yields_nothing(self, tmp_path) -> None:
        assert _workspace_crate_names(tmp_path) == set()
