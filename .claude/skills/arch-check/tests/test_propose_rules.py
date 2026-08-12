"""The rule proposer — every candidate has to come from a measurement.

The tests here exist to pin two opposite failures. Proposing a rule the repo does not already
obey ships a config that fails on its first run, which teaches everyone to ignore it. Proposing
nothing when the graph is real wastes the only cheap moment to adopt boundaries. The refusal
tests are the important ones: a proposer that answers confidently about a codebase it never read
is the same defect the D5 meta-gate exists to catch.
"""
from __future__ import annotations

import sys
from pathlib import Path

_SCRIPTS = Path(__file__).resolve().parent.parent / "scripts"
if str(_SCRIPTS) not in sys.path:
    sys.path.insert(0, str(_SCRIPTS))

from propose_rules import (  # noqa: E402
    Graph,
    allow_list,
    _dynamic_imports,
    _export_target,
    _workspace_import,
    _workspace_packages,
    _iter_json_objects,
    _unit_of_import,
    _unit_of_path,
    find_cycles,
    independent_pairs,
    one_way_candidates,
    propose,
)


def _graph(*edges: tuple[str, str, int]) -> Graph:
    g = Graph()
    for source, target, times in edges:
        g.see(source)
        g.see(target)
        for _ in range(times):
            g.add(source, target)
    return g


class TestOneWayCandidates:
    def test_a_one_way_edge_becomes_a_rule(self) -> None:
        [candidate] = one_way_candidates(_graph(("tui", "agents", 33)))
        assert (candidate.source, candidate.target) == ("agents", "tui")
        assert "33 import(s)" in candidate.evidence

    def test_traffic_in_both_directions_proposes_nothing(self) -> None:
        """There is no invariant to freeze — and a rule here would fail on its first run."""
        assert one_way_candidates(_graph(("a", "b", 5), ("b", "a", 1))) == []

    def test_the_rule_forbids_the_direction_that_is_empty(self) -> None:
        """Getting this backwards would forbid the 33 imports that actually exist."""
        [candidate] = one_way_candidates(_graph(("tui", "agents", 33)))
        assert candidate.source == "agents", "must forbid agents importing tui, not the reverse"

    def test_a_self_edge_is_not_a_dependency(self) -> None:
        assert one_way_candidates(_graph(("a", "a", 9))) == []


class TestAllowList:
    """The rigorous form: what each unit imports today, everything else forbidden."""

    def test_each_unit_lists_what_it_actually_imports(self) -> None:
        assert allow_list(_graph(("cmd", "internal/auth", 3))) == {
            "cmd": ["internal/auth"],
            "internal/auth": [],
        }

    def test_a_unit_that_imports_nothing_gets_an_empty_list_not_absence(self) -> None:
        """Absent would read as unconstrained; empty says 'imports nothing', which is the claim."""
        assert allow_list(_graph(("a", "b", 1)))["b"] == []

    def test_it_scales_where_enumerating_pairs_does_not(self) -> None:
        """theo-cloud has 27 units — 702 ordered pairs, 302 of which never touch. A proposal of
        302 rules is skimmed and dismissed; 27 allow-lists is read."""
        graph = Graph()
        for i in range(27):
            graph.see(f"u{i}")
        graph.add("u0", "u1")
        assert len(allow_list(graph)) == 27


class TestIndependentPairs:
    def test_two_participating_top_level_units_that_never_meet_are_siblings(self) -> None:
        graph = _graph(("tui", "agents", 3), ("exec", "agents", 2))
        pairs = {(c.source, c.target) for c in independent_pairs(graph)}
        assert ("exec", "tui") in pairs

    def test_deep_units_are_not_proposed_as_siblings(self) -> None:
        """Sparsity between packages is not an invariant, and it scales as N^2. The allow-list
        already forbids the unlisted edge, so only the noise is lost."""
        graph = _graph(("internal/auth", "internal/account", 2), ("internal/billing", "internal/account", 1))
        assert independent_pairs(graph) == []

    def test_units_that_do_exchange_imports_are_not_siblings(self) -> None:
        graph = _graph(("tui", "exec", 1), ("exec", "agents", 2))
        assert ("exec", "tui") not in {(c.source, c.target) for c in independent_pairs(graph)}

    def test_a_unit_outside_the_graph_yields_no_pair(self) -> None:
        """Two directories with no edges at all are unrelated, not siblings — a rule between them
        would govern nothing, and D5 would then report it as vacuous."""
        graph = _graph(("a", "b", 1))
        graph.see("orphan")
        assert all("orphan" not in (c.source, c.target) for c in independent_pairs(graph))


class TestFindCycles:
    def test_an_acyclic_graph_has_none(self) -> None:
        assert find_cycles(_graph(("a", "b", 1), ("b", "c", 1))) == []

    def test_a_two_unit_cycle_is_found(self) -> None:
        assert find_cycles(_graph(("a", "b", 1), ("b", "a", 1))) == [("a", "b")]

    def test_a_longer_cycle_is_found(self) -> None:
        cycles = find_cycles(_graph(("a", "b", 1), ("b", "c", 1), ("c", "a", 1)))
        assert cycles == [("a", "b", "c")]


class TestPropose:
    def test_an_unread_repo_is_refused_not_answered(self) -> None:
        """The failure this whole file guards against: tree-sitter degrades to an empty result,
        so an empty graph is reachable without any error being raised."""
        result = propose(Graph())
        assert result["status"] == "refused"
        assert result["candidates"] == []

    def test_units_seen_but_no_imports_is_independence_not_refusal(self) -> None:
        """theo-contracts, measured: jwt, plan and serviceauth import none of each other."""
        graph = Graph()
        for unit in ("jwt", "plan", "serviceauth"):
            graph.see(unit)
        result = propose(graph)
        assert result["status"] == "proposed"
        assert [c["kind"] for c in result["candidates"]] == ["independence"]

    def test_one_unit_alone_is_refused(self) -> None:
        graph = Graph()
        graph.see("only")
        assert propose(graph)["status"] == "refused"

    def test_no_circular_is_proposed_when_there_are_no_cycles(self) -> None:
        result = propose(_graph(("a", "b", 1)))
        assert result["candidates"][0]["kind"] == "no-circular"

    def test_no_circular_is_withheld_when_a_cycle_exists(self) -> None:
        """Adopting it here would ship a config that is red on arrival."""
        result = propose(_graph(("a", "b", 1), ("b", "a", 1)))
        assert all(c["kind"] != "no-circular" for c in result["candidates"])
        assert result["not_proposed"][0]["kind"] == "no-circular"
        assert "backlog" in result["not_proposed"][0]["reason"]

    def test_every_candidate_carries_its_measurement(self) -> None:
        """A rule without the number that justifies it is the rule the next refactor deletes."""
        result = propose(_graph(("tui", "agents", 4), ("exec", "agents", 2)))
        for candidate in result["candidates"]:
            assert candidate["evidence"].strip(), candidate


class TestUnitResolution:
    def test_an_import_outside_the_module_is_not_a_unit(self) -> None:
        assert _unit_of_import("github.com/other/x/y", "github.com/us/repo") == ""

    def test_the_first_segment_under_the_module_is_the_unit(self) -> None:
        assert _unit_of_import("github.com/us/repo/internal/db", "github.com/us/repo") == "internal"

    def test_the_module_root_is_its_own_unit(self) -> None:
        """This test used to assert `""`, pinning the defect: the package at the module root got
        no unit, so no component covered it, so `main.go` and `tools.go` were governed by nothing
        while the config reported full coverage. 22 files across three of theo's modules."""
        assert _unit_of_import("github.com/us/repo", "github.com/us/repo") == "."

    def test_a_vendored_path_is_not_a_unit(self) -> None:
        assert _unit_of_import("github.com/us/repo/vendor/x", "github.com/us/repo") == ""

    def test_a_file_at_the_root_has_no_unit(self, tmp_path: Path) -> None:
        assert _unit_of_path(tmp_path / "main.ts", tmp_path) == ""

    def test_a_file_in_a_directory_takes_that_directory(self, tmp_path: Path) -> None:
        assert _unit_of_path(tmp_path / "agents" / "goal" / "x.ts", tmp_path) == "agents"

    def test_a_path_outside_the_root_has_no_unit(self, tmp_path: Path) -> None:
        assert _unit_of_path(Path("/elsewhere/x.ts"), tmp_path) == ""


class TestGoListParsing:
    def test_concatenated_objects_are_all_read(self) -> None:
        """`go list -json` emits objects back to back, not a JSON array."""
        assert list(_iter_json_objects('{"a":1}\n{"a":2}\n')) == [{"a": 1}, {"a": 2}]

    def test_a_truncated_stream_stops_instead_of_raising(self) -> None:
        assert list(_iter_json_objects('{"a":1}\n{"a":')) == [{"a": 1}]

    def test_an_empty_stream_yields_nothing(self) -> None:
        assert list(_iter_json_objects("")) == []


class TestUnitGranularity:
    """A directory with no sources of its own groups; it does not implement."""

    PACKAGES = frozenset({"cmd", "internal/auth", "internal/account", "internal/auth/oauth"})
    MOD = "github.com/us/repo"

    def test_a_container_directory_is_descended_past(self) -> None:
        """`internal/` holds 0 Go files and 28 subdirectories in theo-cloud. Collapsing them into
        one unit hid every dependency between them and left 44 packages ungoverned."""
        assert _unit_of_import(f"{self.MOD}/internal/auth", self.MOD, self.PACKAGES) == "internal/auth"

    def test_a_real_package_at_the_top_stays_at_the_top(self) -> None:
        assert _unit_of_import(f"{self.MOD}/cmd", self.MOD, self.PACKAGES) == "cmd"

    def test_a_subpackage_collapses_into_its_owning_unit(self) -> None:
        """`internal/auth/oauth` belongs to `internal/auth` — the smallest prefix that is a
        package. Otherwise every leaf becomes its own architectural unit."""
        assert (
            _unit_of_import(f"{self.MOD}/internal/auth/oauth", self.MOD, self.PACKAGES)
            == "internal/auth"
        )

    def test_without_a_package_list_it_falls_back_to_the_first_segment(self) -> None:
        assert _unit_of_import(f"{self.MOD}/internal/auth", self.MOD) == "internal"

    def test_node_modules_at_any_depth_is_not_a_unit(self) -> None:
        """A Go file vendored inside a TypeScript app's node_modules became an architectural unit
        of theo-cloud, because only the first segment was checked."""
        assert (
            _unit_of_import(
                f"{self.MOD}/dashboard/node_modules/flatted/golang", self.MOD, self.PACKAGES
            )
            == ""
        )


class TestGoStrictFilter:
    """Filtering Go packages by NAME deletes legitimate ones."""

    MOD = "github.com/us/repo"

    def test_a_package_named_build_survives(self) -> None:
        """`theo/api/internal/services/build` is a real Go package. The filesystem-walk filter
        contains 'build', and applying it here deleted the package: 14 violations against a
        component that no longer existed, plus 21 files governed by nothing."""
        assert (
            _unit_of_import(f"{self.MOD}/internal/services/build", self.MOD, frozenset({"internal/services/build"}))
            == "internal/services/build"
        )

    def test_a_package_named_docs_or_scripts_survives(self) -> None:
        for name in ("docs", "scripts", "dist", "examples"):
            packages = frozenset({f"internal/{name}"})
            assert _unit_of_import(f"{self.MOD}/internal/{name}", self.MOD, packages) == f"internal/{name}"

    def test_vendor_and_node_modules_are_still_refused(self) -> None:
        """The three that can never be a Go package of this module stay out."""
        for name in ("vendor", "node_modules", "testdata"):
            assert _unit_of_import(f"{self.MOD}/{name}/x", self.MOD, frozenset()) == ""

    def test_the_module_root_package_is_a_unit(self) -> None:
        """`main.go` at the module root belonged to no component: 22 files across three of theo's
        modules sat outside every rule while the config reported full coverage."""
        assert _unit_of_import(self.MOD, self.MOD, frozenset()) == "."


class TestWorkspaceMonorepo:
    """A monorepo's cross-unit edges are BARE specifiers, and skipping them inverted the answer.

    Measured on TheoCode: 4 packages exchanging 80 imports were reported as 0 edges, and the
    proposer then offered `independence` — a rule forbidding all 80. Every one of these tests
    pins a step of that failure.
    """

    @staticmethod
    def _repo(tmp_path: Path, *, workspaces=("packages/*",)) -> Path:
        import json as _json

        (tmp_path / "package.json").write_text(_json.dumps({"workspaces": list(workspaces)}))
        for name, exports in (
            ("agent", {".": "./src/index.ts", "./config": "./src/config/index.ts"}),
            # `shared` declares NO `.` entry — a layout convention like "<pkg>/src/index.ts"
            # would have resolved nothing for it. This is TheoCode's real shape.
            ("shared", {"./shutdown": "./src/shutdown.ts"}),
        ):
            pkg = tmp_path / "packages" / name
            (pkg / "src").mkdir(parents=True)
            (pkg / "package.json").write_text(
                _json.dumps({"name": f"@theocode/{name}", "exports": exports})
            )
        return tmp_path

    def test_workspace_packages_are_read_from_the_root_manifest(self, tmp_path: Path) -> None:
        found = _workspace_packages(self._repo(tmp_path))
        assert set(found) == {"@theocode/agent", "@theocode/shared"}

    def test_a_repo_declaring_no_workspaces_finds_none(self, tmp_path: Path) -> None:
        """Without the declaration there is nothing separating `@theocode/agent` from `react`,
        and guessing from the `@scope/` prefix would invent architecture from a naming habit."""
        (tmp_path / "package.json").write_text('{"name": "solo"}')
        assert _workspace_packages(tmp_path) == {}

    def test_a_subpath_export_resolves_to_its_file(self, tmp_path: Path) -> None:
        repo = self._repo(tmp_path)
        target = _export_target(repo / "packages" / "shared", "shutdown")
        assert target == (repo / "packages" / "shared" / "src" / "shutdown.ts").resolve()

    def test_a_scoped_name_is_not_split_on_its_first_slash(self, tmp_path: Path) -> None:
        """`@theocode/agent/config` splits into `@theocode/agent` + `config`. Splitting on the
        first `/` yields `@theocode`, which matches no declared package, and the edge vanishes."""
        repo = self._repo(tmp_path)
        graph = Graph()
        resolved = _workspace_import("@theocode/agent/config", _workspace_packages(repo), graph)
        assert resolved == (repo / "packages" / "agent" / "src" / "config" / "index.ts").resolve()
        assert graph.unresolved_workspace == set()

    def test_a_third_party_specifier_stays_out(self, tmp_path: Path) -> None:
        graph = Graph()
        assert _workspace_import("react", _workspace_packages(self._repo(tmp_path)), graph) is None
        assert graph.unresolved_workspace == set()

    def test_an_unresolvable_workspace_specifier_is_recorded_not_dropped(self, tmp_path: Path) -> None:
        """It names a package of this repo, so the edge EXISTS. Dropping it silently is what let
        a zero-edge graph pass for independence."""
        graph = Graph()
        _workspace_import("@theocode/shared/nope", _workspace_packages(self._repo(tmp_path)), graph)
        assert graph.unresolved_workspace == {"@theocode/shared/nope"}

    def test_zero_edges_with_an_unresolved_specifier_is_refused(self) -> None:
        """The fingerprint of `independence` and the fingerprint of a broken resolver are the
        same edge count. Only one of them permits a conclusion."""
        graph = Graph(units_seen={"a", "b", "c"}, unresolved_workspace={"@x/y"})
        result = propose(graph)
        assert result["status"] == "refused"
        assert result["unresolved_workspace_imports"] == ["@x/y"]

    def test_zero_edges_with_nothing_unresolved_is_still_independence(self) -> None:
        """The refusal must not swallow the real finding. theo-contracts measured exactly this."""
        assert propose(Graph(units_seen={"a", "b", "c"}))["status"] == "proposed"


class TestDynamicImports:
    """`await import('x')` is a call expression, so the statement-level extractor never sees it."""

    def test_a_dynamic_import_is_read(self, tmp_path: Path) -> None:
        """17 of TheoCode's 23 `cli -> agent` crossings are dynamic. Reading only static imports
        under-reported that edge by 74%."""
        f = tmp_path / "a.ts"
        f.write_text("const { x } = await import('@theocode/agent/auth')\n")
        assert _dynamic_imports(f) == ["@theocode/agent/auth"]

    def test_a_relative_dynamic_import_is_read(self, tmp_path: Path) -> None:
        f = tmp_path / "a.ts"
        f.write_text("const m = await import(\"../shared/thing\")\n")
        assert _dynamic_imports(f) == ["../shared/thing"]

    def test_a_computed_specifier_is_missed_and_that_is_the_safe_direction(self, tmp_path: Path) -> None:
        """`import(someVar)` is unreadable by any static means. Missing it under-reports an edge,
        which at worst withholds a rule; inventing one would propose a false boundary."""
        f = tmp_path / "a.ts"
        f.write_text("const m = await import(modulePath)\n")
        assert _dynamic_imports(f) == []

    def test_the_word_import_in_a_statement_is_not_matched_as_a_call(self, tmp_path: Path) -> None:
        f = tmp_path / "a.ts"
        f.write_text("import { x } from '@theocode/agent'\n")
        assert _dynamic_imports(f) == []
