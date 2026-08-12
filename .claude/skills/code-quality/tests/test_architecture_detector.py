"""D5 — the architecture detector and its meta-gate.

The tests that matter here are the ones about a rule that CANNOT FAIL. Everything else in this
file is ordinary plumbing; the vacuity cases are the reason D5 exists, because they are the ones
where the tool under D5 reports success.
"""
from __future__ import annotations

import json
from pathlib import Path

from scripts.detectors import _arch
from scripts.detectors.go import GoDetector, _component_of
from scripts.detectors.python import PythonDetector
from scripts.detectors.rust import RustDetector, _setup_blocker
from scripts.detectors.typescript import (
    TypescriptDetector,
    _depcruise_script,
    _path_tokens,
    _rule_selfcheck,
)


# ---------------------------------------------------------------------------
# The shared vocabulary
# ---------------------------------------------------------------------------


class TestReasonSubstance:
    """A rule's reason has to say something. `n/a` is how you decline to answer."""

    def test_a_written_reason_passes(self) -> None:
        assert _arch.reason_has_substance("Domain must not import the surface that consumes it.")

    def test_the_three_ways_of_not_answering_fail(self) -> None:
        for non_answer in ("n/a", "-", "interno", "", "   "):
            assert not _arch.reason_has_substance(non_answer)

    def test_absent_is_not_an_answer(self) -> None:
        assert not _arch.reason_has_substance(None)

    def test_the_threshold_is_declared_not_magic(self) -> None:
        assert _arch.RAZAO_MINIMA == 20
        assert not _arch.reason_has_substance("x" * (_arch.RAZAO_MINIMA - 1))
        assert _arch.reason_has_substance("x" * _arch.RAZAO_MINIMA)


class TestSeverities:
    """A vacuous rule is as severe as a broken one: the invariant is gone either way."""

    def test_a_vacuous_rule_is_hard(self) -> None:
        f = _arch.vacuous_rule("go", tool="t", rule="r", config_path="c.yml", detail="d")
        assert f.severity == "HARD"

    def test_a_violation_is_hard(self) -> None:
        f = _arch.violation("go", tool="t", rule="r", file_path="a.go", symbol_or_line="1", message="m")
        assert f.severity == "HARD"

    def test_an_unexplained_rule_only_caps(self) -> None:
        """The boundary still holds — what is missing is the next reader's ability to keep it."""
        f = _arch.rule_without_reason("go", tool="t", rule="r", config_path="c.yml")
        assert f.severity == "SOFT_FLOOR"

    def test_no_config_is_info_not_failure(self) -> None:
        """Squad has no opinion on a repo's layers until the repo writes one down."""
        f = _arch.no_config("go", tool="t", looked_for=["x.yml"])
        assert f.severity == "INFO"

    def test_unavailable_auditor_is_soft_cap(self) -> None:
        assert _arch.auditor_unavailable("go", tool="t", reason="r").severity == "SOFT_CAP"


# ---------------------------------------------------------------------------
# TypeScript — dependency-cruiser
# ---------------------------------------------------------------------------


class TestPathTokens:
    def test_extracts_directory_shaped_literals(self) -> None:
        assert _path_tokens("^(tui|exec)/") == []  # no `/` inside a token, nothing to check
        assert "agents/lib" in _path_tokens("^agents/lib/")

    def test_skips_single_words(self) -> None:
        """`index` in `(?!index)` is a filename fragment; flagging it makes the gate noisy."""
        assert _path_tokens("^agents/([^/]+)/(?!index)") == ["agents"] or _path_tokens(
            "^agents/([^/]+)/(?!index)"
        ) == []

    def test_a_non_string_pattern_yields_nothing(self) -> None:
        assert _path_tokens(None) == []
        assert _path_tokens({"circular": True}) == []


class TestRuleSelfcheck:
    """The `tui/lib` incident, in test form."""

    SOURCES = {"agents/goal/goal.ts", "tui/consent/approval-mode.ts", "exec/main.ts"}

    def _rule(self, **over: object) -> dict:
        rule = {
            "name": "regra",
            "comment": "Uma razao com substancia suficiente para passar o limiar.",
            "from": {"path": "^agents/goal/"},
            "to": {"path": "^tui/consent/"},
        }
        rule.update(over)  # type: ignore[arg-type]
        return rule

    def test_a_rule_whose_from_still_exists_is_silent(self) -> None:
        assert _rule_selfcheck({"forbidden": [self._rule()]}, self.SOURCES) == []

    def test_a_rule_governing_a_deleted_directory_is_hard(self) -> None:
        """`from` selects the code the rule governs. Gone means the rule can never fire."""
        findings = _rule_selfcheck(
            {"forbidden": [self._rule(**{"from": {"path": "^tui/lib/"}})]}, self.SOURCES
        )
        assert [f.severity for f in findings] == ["HARD"]
        assert "tui/lib" in findings[0].message

    def test_a_rule_targeting_a_deleted_directory_only_caps(self) -> None:
        """`to` may legitimately match nothing — that is a preventive rule doing its job."""
        findings = _rule_selfcheck(
            {"forbidden": [self._rule(to={"path": "^tui/lib/"})]}, self.SOURCES
        )
        assert [f.severity for f in findings] == ["SOFT_FLOOR"]

    def test_an_external_target_is_never_flagged(self) -> None:
        """`no-sdk-direto` forbids an import nobody makes. Demanding a match would invert it."""
        findings = _rule_selfcheck(
            {"forbidden": [self._rule(to={"path": "^node_modules/@theokit/sdk"})]}, self.SOURCES
        )
        assert findings == []

    def test_a_rule_without_a_reason_is_reported(self) -> None:
        findings = _rule_selfcheck({"forbidden": [self._rule(comment="n/a")]}, self.SOURCES)
        assert [f.severity for f in findings] == ["SOFT_FLOOR"]
        assert "no reason" in findings[0].message

    def test_an_empty_rule_set_yields_nothing(self) -> None:
        assert _rule_selfcheck({}, self.SOURCES) == []


class TestDepcruiseScript:
    def test_finds_the_script_that_runs_the_tool(self, tmp_path: Path) -> None:
        pkg = tmp_path / "package.json"
        pkg.write_text(json.dumps({"scripts": {"test": "vitest", "boundaries": "depcruise --config x src"}}))
        assert _depcruise_script(pkg) == "boundaries"

    def test_absent_when_no_script_runs_it(self, tmp_path: Path) -> None:
        pkg = tmp_path / "package.json"
        pkg.write_text(json.dumps({"scripts": {"test": "vitest"}}))
        assert _depcruise_script(pkg) is None

    def test_malformed_package_json_is_absent_not_a_crash(self, tmp_path: Path) -> None:
        pkg = tmp_path / "package.json"
        pkg.write_text("{ not json")
        assert _depcruise_script(pkg) is None


class TestTypescriptDetector:
    def test_a_zero_module_cruise_is_vacuous_not_clean(self) -> None:
        """Measured in usetheo-labs/agent-builder: the global binary cruised 0 modules against a
        config the local one cruised 279 with. Zero violations over zero modules is not a pass."""
        findings = TypescriptDetector()._parse_depcruise_json(
            {"summary": {"totalCruised": 0, "violations": [], "ruleSetUsed": {}}, "modules": []},
            "boundaries",
        )
        assert [f.severity for f in findings] == ["HARD"]
        assert "0 modules" in findings[0].message

    def test_an_error_violation_is_hard_and_a_warn_is_not(self) -> None:
        payload = {
            "summary": {
                "totalCruised": 10,
                "ruleSetUsed": {},
                "violations": [
                    {"rule": {"name": "r1", "severity": "error"}, "from": "a.ts", "to": "b.ts"},
                    {"rule": {"name": "r2", "severity": "warn"}, "from": "c.ts", "to": "d.ts"},
                ],
            },
            "modules": [{"source": "a.ts"}],
        }
        severities = [f.severity for f in TypescriptDetector()._parse_depcruise_json(payload, "b")]
        assert severities == ["HARD", "SOFT_FLOOR"]

    def test_a_payload_without_summary_reports_unavailable(self) -> None:
        findings = TypescriptDetector()._parse_depcruise_json({"modules": []}, "b")
        assert [f.severity for f in findings] == ["SOFT_CAP"]

    def test_no_package_json_is_a_skip(self, tmp_path: Path) -> None:
        findings = TypescriptDetector().detect_architecture_violations(tmp_path)
        assert [f.severity for f in findings] == ["INFO"]

    def test_tsarch_declared_and_never_imported_is_reported(self, tmp_path: Path) -> None:
        """theo#255 catalogued this shape nine times: a gate registered and never run."""
        (tmp_path / "package.json").write_text(
            json.dumps({"devDependencies": {"tsarch": "^5.4.1"}, "scripts": {}})
        )
        (tmp_path / "some.ts").write_text("export const x = 1\n")
        findings = TypescriptDetector().detect_architecture_violations(tmp_path)
        assert any(f.symbol_or_line == "tsarch" and f.severity == "SOFT_FLOOR" for f in findings)

    def test_tsarch_actually_used_is_silent(self, tmp_path: Path) -> None:
        (tmp_path / "package.json").write_text(
            json.dumps({"devDependencies": {"tsarch": "^5.4.1"}, "scripts": {}})
        )
        (tmp_path / "arch.test.ts").write_text("import { filesOfProject } from 'tsarch'\n")
        findings = TypescriptDetector().detect_architecture_violations(tmp_path)
        assert not any(f.symbol_or_line == "tsarch" for f in findings)


# ---------------------------------------------------------------------------
# Go — go-arch-lint
# ---------------------------------------------------------------------------


class TestGoArchParsing:
    def test_component_name_is_pulled_out_of_the_warning(self) -> None:
        text = "not found directories for 'billing' in '/repo/billing'"
        assert _component_of(text) == "billing"

    def test_an_unparseable_warning_yields_a_placeholder(self) -> None:
        assert _component_of("something else entirely") == "?"

    def test_a_ghost_component_is_hard_even_though_the_tool_says_no_warnings(self) -> None:
        """Measured on theo-contracts 2026-08-06. `ArchHasWarnings: false` — green — while the
        component's directory does not exist and the rule can no longer fire."""
        payload = {
            "Payload": {
                "ArchHasWarnings": False,
                "ExecutionWarnings": [
                    {"Text": "not found directories for 'fantasma' in '/repo/fantasma'"}
                ],
                "ArchWarningsDeps": [],
                "ArchWarningsNotMatched": [],
            }
        }
        findings = GoDetector()._parse_arch_json(payload)
        assert [f.severity for f in findings] == ["HARD"]
        assert findings[0].symbol_or_line == "fantasma"

    def test_a_dependency_violation_is_hard(self) -> None:
        payload = {
            "Payload": {
                "ArchWarningsDeps": [
                    {
                        "ComponentName": "jwt",
                        "FileRelativePath": "/jwt/a.go",
                        "ResolvedImportName": "example.com/plan",
                        "Reference": {"Line": 12},
                    }
                ]
            }
        }
        findings = GoDetector()._parse_arch_json(payload)
        assert [f.severity for f in findings] == ["HARD"]
        assert findings[0].file_path == "jwt/a.go"

    def test_packages_matched_by_no_component_only_cap(self) -> None:
        payload = {"Payload": {"ArchWarningsNotMatched": [{"x": 1}, {"x": 2}]}}
        findings = GoDetector()._parse_arch_json(payload)
        assert [f.severity for f in findings] == ["SOFT_FLOOR"]
        assert "2 package(s)" in findings[0].message

    def test_a_payload_without_the_envelope_reports_unavailable(self) -> None:
        assert [f.severity for f in GoDetector()._parse_arch_json({"nope": 1})] == ["SOFT_CAP"]

    def test_no_config_is_a_skip(self, tmp_path: Path) -> None:
        assert [f.severity for f in GoDetector().detect_architecture_violations(tmp_path)] == ["INFO"]

    def test_disabling_the_tools_own_guard_is_hard(self, tmp_path: Path) -> None:
        """`ignoreNotFoundComponents: true` silences exactly the failure D5 exists to catch."""
        config = tmp_path / ".go-arch-lint.yml"
        config.write_text("version: 3\nallow:\n  ignoreNotFoundComponents: true\n")
        findings = GoDetector()._arch_config_selfcheck(config)
        assert [f.severity for f in findings] == ["HARD"]

    def test_the_guard_left_on_is_silent(self, tmp_path: Path) -> None:
        config = tmp_path / ".go-arch-lint.yml"
        config.write_text("version: 3\nallow:\n  ignoreNotFoundComponents: false\n")
        assert GoDetector()._arch_config_selfcheck(config) == []

    def test_a_commented_out_setting_does_not_count(self, tmp_path: Path) -> None:
        config = tmp_path / ".go-arch-lint.yml"
        config.write_text("version: 3\n# ignoreNotFoundComponents: true\n")
        assert GoDetector()._arch_config_selfcheck(config) == []


# ---------------------------------------------------------------------------
# Rust — layered-crate
# ---------------------------------------------------------------------------


class TestRustSetupBlockers:
    """Each string below was observed against theo-db on 2026-08-06."""

    def test_missing_lib_path_is_a_setup_blocker(self) -> None:
        assert _setup_blocker("fatal: failed to read lib.path from Cargo.toml") is not None

    def test_workspace_collision_is_a_setup_blocker(self) -> None:
        assert _setup_blocker("error: two packages named `theodb_rs` in this workspace") is not None

    def test_missing_pgrx_is_a_setup_blocker(self) -> None:
        assert _setup_blocker("Error: /home/x/.pgrx/config.toml not found.") is not None

    def test_an_ordinary_compile_error_is_not_a_setup_blocker(self) -> None:
        """Otherwise a real layer violation would be filed as 'tool unavailable' and disappear."""
        assert _setup_blocker("error[E0432]: unresolved import `crate::db`") is None

    def test_no_layerfile_is_a_skip(self, tmp_path: Path) -> None:
        assert [f.severity for f in RustDetector().detect_architecture_violations(tmp_path)] == ["INFO"]


# ---------------------------------------------------------------------------
# Python — import-linter
# ---------------------------------------------------------------------------


class TestPythonDetector:
    def test_no_config_at_all_is_a_skip(self, tmp_path: Path) -> None:
        assert [f.severity for f in PythonDetector().detect_architecture_violations(tmp_path)] == ["INFO"]

    def test_a_config_without_an_importlinter_section_is_a_skip(self, tmp_path: Path) -> None:
        (tmp_path / "setup.cfg").write_text("[metadata]\nname = x\n")
        findings = PythonDetector().detect_architecture_violations(tmp_path)
        assert [f.severity for f in findings] == ["INFO"]
        assert "importlinter" in findings[0].message
