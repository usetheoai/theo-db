"""Rendering a proposal into a linter config — where the criterion gets lost.

Adopting a measured proposal against theo-cloud by hand took four attempts, and every failure was
a translation detail rather than a wrong rule. Each of those failures has a test here, because
each of them made the config report success while enforcing less than it claimed.
"""
from __future__ import annotations

import sys
from pathlib import Path

_SCRIPTS = Path(__file__).resolve().parent.parent / "scripts"
if str(_SCRIPTS) not in sys.path:
    sys.path.insert(0, str(_SCRIPTS))

import pytest  # noqa: E402

from emit_config import _component, go_arch_lint, split_by_module  # noqa: E402

_PROPOSAL = {
    "status": "proposed",
    "language": "go",
    "total_edges": 24,
    "cycles": [],
    "allow_list": {"cmd": ["internal/auth"], "internal/auth": [], ".": []},
}


def _render() -> str:
    return go_arch_lint(_PROPOSAL, measured_on="2026-08-06")


class TestComponentNames:
    def test_a_path_becomes_a_flat_identifier(self) -> None:
        assert _component("internal/auth") == "internal_auth"

    def test_the_module_root_is_named_root(self) -> None:
        assert _component(".") == "root"

    def test_a_hyphen_becomes_an_underscore(self) -> None:
        assert _component("cmd/theo-ops") == "cmd_theo_ops"


class TestRenderedConfig:
    def test_every_component_key_carries_its_colon(self) -> None:
        """Omitting it produced 32 schema errors filed under ExecutionWarnings — a field nothing
        reads — while ArchWarningsDeps said 0. Green, having validated nothing."""
        for line in _render().splitlines():
            if line.startswith("  ") and "{ in:" in line:
                assert ":" in line.split("{")[0], line

    def test_a_component_covers_its_subtree_as_well_as_itself(self) -> None:
        """`in: internal/auth` alone left 24 packages matched by no component."""
        assert "{ in: [internal/auth, internal/auth/**] }" in _render()

    def test_the_root_component_does_not_swallow_the_module(self) -> None:
        """`in: .` covers the root's own files; adding `./**` would absorb every package."""
        rendered = _render()
        assert "{ in: . }" in rendered
        assert "./**" not in rendered

    def test_every_unit_may_depend_on_itself(self) -> None:
        """A unit importing its own subpackages is cohesion, not a boundary crossing. Forbidding
        it produced 63 violations that were not crossings at all."""
        assert "mayDependOn: [internal_auth]" in _render()

    def test_glob_excludes_are_quoted(self) -> None:
        """A scalar starting with `*` is a YAML alias, so an unquoted `**/node_modules` parses to
        null and excludes nothing — three `given: null` warnings, silently."""
        for line in _render().splitlines():
            if line.strip().startswith("- ") and "node_modules" in line:
                assert line.strip().startswith('- "'), line

    def test_tests_are_excluded_so_the_enforced_set_matches_the_measured_one(self) -> None:
        """The allow-list comes from production imports; the linter scans tests too. 49 of
        theo-cloud's violations came from `_test.go` files alone."""
        assert "excludeFiles:" in _render()
        assert "_test" in _render()

    def test_an_empty_allow_list_is_refused(self) -> None:
        with pytest.raises(ValueError, match="nothing to enforce"):
            go_arch_lint({**_PROPOSAL, "allow_list": {}}, measured_on="x")


class TestSplitByModule:
    """go-arch-lint reads one `go.mod` at a time: pointed at a workspace root it finds no project."""

    PROPOSAL = {
        "allow_list": {
            "api/cmd": ["api/internal"],
            "api/internal": [],
            "pkg/store": [],
        },
        "modules": ["api", "pkg"],
    }

    def test_each_module_gets_its_own_allow_list(self) -> None:
        assert set(split_by_module(self.PROPOSAL)) == {"api", "pkg"}

    def test_the_module_prefix_is_stripped_from_units(self) -> None:
        assert split_by_module(self.PROPOSAL)["api"]["allow_list"] == {
            "cmd": ["internal"],
            "internal": [],
        }

    def test_a_single_module_repo_is_left_whole(self) -> None:
        assert list(split_by_module({"allow_list": {"cmd": []}})) == [""]

    def test_a_cross_module_target_is_dropped_not_renamed(self) -> None:
        """`api` importing `pkg` is a dependency between MODULES; go-arch-lint sees it as a vendor
        import, and inventing a component for it here would produce a rule about code the config's
        module does not contain."""
        proposal = {"allow_list": {"api/cmd": ["pkg/store"]}, "modules": ["api", "pkg"]}
        assert split_by_module(proposal)["api"]["allow_list"] == {"cmd": []}
