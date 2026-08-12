#!/usr/bin/env python3
"""Derive architecture rules a repo ALREADY obeys, so adopting them costs nothing.

## The problem this solves

Writing architecture rules for a repo you did not build means deciding what its architecture is.
Do that from taste and you produce a gate that reports violations which are evidence of nothing —
the defect gate G5 rejects when prior art is offered as justification.

## The criterion

An edge that is already one-way is a measured invariant, not an opinion. If `tui/` imports
`agents/` forty times and `agents/` imports `tui/` zero times, then "agents must not import tui"
is *already true*; writing it down changes no code and freezes a property the repo has. If both
directions carry traffic, there is no invariant to freeze and this tool proposes nothing.

usetheo-labs/agent-builder states the same rule of adoption from the other side:

    "Nenhuma destas regras foi escrita contra violacao existente: as cinco sairam de 0 violacoes
     no commit que as introduziu, o que significa que elas CONGELAM um estado bom em vez de
     anunciar divida."

So a candidate that would fail on day one is NOT proposed as a gate. It is a finding — someone
has to decide whether the crossing is a defect or the architecture — and that decision belongs in
the backlog, not in a config file.

## The guard that matters most

An empty graph would make every pair look one-way and this tool would propose a full rule set
built on having parsed nothing. `extract_imports_and_calls` degrades to an empty list when
tree-sitter is missing, so that failure is reachable. `propose` refuses on an empty graph rather
than answering confidently about a codebase it never read.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

_SKILL_ROOT = Path(__file__).resolve().parent.parent
_CODE_QUALITY_SCRIPTS = _SKILL_ROOT.parent / "code-quality"
if str(_CODE_QUALITY_SCRIPTS) not in sys.path:
    sys.path.insert(0, str(_CODE_QUALITY_SCRIPTS))

#: Directories that are never architectural units when WALKING A FILESYSTEM. Used by the
#: TypeScript path, which globs directories and cannot tell a source tree from a build output.
_NOT_A_UNIT = frozenset(
    {
        "node_modules", "vendor", "target", "dist", "build", ".git", ".claude",
        "testdata", "__pycache__", "coverage", "docs", "examples", "scripts",
    }
)

#: The strict set, for Go. `go list` returns real packages only, so a directory it names IS one —
#: and filtering those by NAME is how a legitimate package disappears. Measured on theo/api:
#: `internal/services/build` is a Go package, and the broad list above silently deleted it,
#: producing 14 violations against a component that no longer existed plus 21 ungoverned files.
#: Only names that can never be a Go package of this module survive here.
_NOT_A_GO_UNIT = frozenset({"node_modules", "vendor", "testdata"})

_GO_LIST_TIMEOUT_SEC = 180


@dataclass(frozen=True)
class Edge:
    """One directed dependency between two top-level units, with how often it occurs."""

    source: str
    target: str
    count: int


@dataclass
class Graph:
    """The dependency graph between a repo's top-level units."""

    units: set[str] = field(default_factory=set)
    edges: dict[tuple[str, str], int] = field(default_factory=dict)
    #: Units the scan SAW, whether or not they have edges. This is what separates "we read the
    #: repo and it has no cross-imports" from "we read nothing" — two states that look identical
    #: from the edge count alone, and only one of which permits a conclusion.
    units_seen: set[str] = field(default_factory=set)
    #: Edges that exist ONLY in test files. Kept apart from `edges` so they never widen the
    #: production allow-list — a test crossing a boundary is not a licence for production to.
    test_edges: dict[tuple[str, str], int] = field(default_factory=dict)
    #: Module directories, when the repo is a `go.work` workspace. Empty for a single-module repo.
    #: go-arch-lint reads a project from its `go.mod`, so a workspace needs one config per module.
    modules: list[str] = field(default_factory=list)
    #: Bare specifiers that NAME a declared workspace package but which the resolver could not map
    #: to a file. These are the edges we know exist and could not measure, and their presence is
    #: what makes a zero-edge result untrustworthy rather than a finding — see `propose`.
    unresolved_workspace: set[str] = field(default_factory=set)

    def see(self, unit: str) -> None:
        """Record that the scan reached this unit, edges or not."""
        if unit:
            self.units_seen.add(unit)

    def add(self, source: str, target: str) -> None:
        if source == target or not source or not target:
            return
        self.units.update({source, target})
        self.edges[(source, target)] = self.edges.get((source, target), 0) + 1

    def count(self, source: str, target: str) -> int:
        return self.edges.get((source, target), 0)

    @property
    def total_edges(self) -> int:
        return sum(self.edges.values())


@dataclass(frozen=True)
class Candidate:
    """A rule the repo already obeys, with the measurement that says so."""

    kind: str
    source: str
    target: str
    evidence: str


def one_way_candidates(graph: Graph) -> list[Candidate]:
    """Every ordered pair where traffic flows one way and the reverse is empty.

    The reverse direction being empty is the whole claim. It is checked, not assumed, and the
    count that proves it goes into the evidence string so the rule carries its own justification
    into the config file.
    """
    out: list[Candidate] = []
    for source in sorted(graph.units):
        for target in sorted(graph.units):
            if source == target:
                continue
            forward = graph.count(source, target)
            backward = graph.count(target, source)
            if forward > 0 and backward == 0:
                out.append(
                    Candidate(
                        kind="one-way",
                        source=target,  # the side that must NOT import
                        target=source,  # the side it must not import
                        evidence=(
                            f"{source} -> {target} carries {forward} import(s); "
                            f"{target} -> {source} carries 0. The rule freezes what already holds"
                        ),
                    )
                )
    return out


def allow_list(graph: Graph) -> dict[str, list[str]]:
    """What each unit imports today. Everything absent from its list is forbidden.

    This is the rigorous form, and it is how go-arch-lint and layered-crate express boundaries
    natively: a unit declares what it may depend on, and the linter forbids the rest. Enumerating
    forbidden PAIRS instead is both weaker and unusable — theo-cloud has 27 units, which is 702
    ordered pairs, and 302 of them never touch. A proposal of 302 rules is not adopted; it is
    skimmed and dismissed, and the criterion this tool exists to serve dies with it.

    Stating it as an allow-list also closes the gap the pairwise form leaves open: a NEW edge
    between two units that happen to be unconnected today is refused by default, without anyone
    having predicted the pair.
    """
    allowed: dict[str, set[str]] = {unit: set() for unit in graph.units_seen}
    for (source, target) in graph.edges:
        allowed.setdefault(source, set()).add(target)
    return {unit: sorted(targets) for unit, targets in sorted(allowed.items())}


def independent_pairs(graph: Graph) -> list[Candidate]:
    """Units that carry traffic elsewhere but never to each other.

    Both directions empty is as measured as one direction empty, and it states something stronger:
    these two are siblings, and neither is a library of the other. Written by hand in
    usetheo-labs/agent-builder as `superficies-nao-se-importam`, with the reasoning that code both
    need belongs in a third place — so the rule is what keeps the third place necessary.

    Restricted to units that participate in the graph at all. Two directories with no edges in any
    direction are not siblings, they are unrelated, and a rule between them would govern nothing.
    """
    # Restricted to top-level units. At package granularity "internal/auth does not import
    # internal/billing" is sparsity, not an invariant worth its own rule — and it scales as N^2.
    # A pair of top-level surfaces is different: they are the architectural peers whose future
    # coupling would actually mean something. The allow-list already forbids every unlisted edge,
    # so nothing is lost by not enumerating the deep pairs; only the noise is.
    active = {u for edge in graph.edges for u in edge if "/" not in u}
    out: list[Candidate] = []
    ordered = sorted(active)
    for i, left in enumerate(ordered):
        for right in ordered[i + 1 :]:
            if graph.count(left, right) or graph.count(right, left):
                continue
            out.append(
                Candidate(
                    kind="siblings",
                    source=left,
                    target=right,
                    evidence=(
                        f"{left} and {right} both take part in the graph and exchange 0 imports "
                        "in either direction. Neither is a library of the other; what both need "
                        "belongs somewhere they share"
                    ),
                )
            )
    return out


def find_cycles(graph: Graph) -> list[tuple[str, ...]]:
    """Every directed cycle between units, as sorted tuples so duplicates collapse.

    A cycle is the one finding here that is never a matter of architecture taste: the Acyclic
    Dependencies Principle does not have a "unless you meant it" clause. Its presence is why
    `no-circular` is proposed only when the count is zero — proposing it against existing cycles
    would ship a config that fails on the first run.
    """
    seen: set[tuple[str, ...]] = set()
    path: list[str] = []
    on_path: set[str] = set()

    def walk(unit: str) -> None:
        if unit in on_path:
            cycle = path[path.index(unit) :]
            seen.add(tuple(sorted(cycle)))
            return
        if len(path) > len(graph.units):
            return
        path.append(unit)
        on_path.add(unit)
        for (source, target) in graph.edges:
            if source == unit:
                walk(target)
        path.pop()
        on_path.discard(unit)

    for unit in sorted(graph.units):
        walk(unit)
    return sorted(seen)


def propose(graph: Graph) -> dict:
    """Turn a measured graph into rule candidates, or refuse if there is no graph.

    The refusal is not politeness. A tool that answers confidently about a codebase it failed to
    read is the exact failure mode the D5 meta-gate exists to catch, and it would be absurd to
    build that gate and then ship it inside this.
    """
    if graph.total_edges == 0:
        if len(graph.units_seen) < 2:
            return {
                "status": "refused",
                "reason": (
                    f"the scan reached {len(graph.units_seen)} unit(s) and found no imports at "
                    "all. Either the parser did not run — tree-sitter degrades to an empty "
                    "result rather than failing — or there is nothing here to govern. Proposing "
                    "from an empty graph would make every pair look one-way, so nothing is "
                    "proposed until the graph is real."
                ),
                "units": sorted(graph.units_seen),
                "candidates": [],
            }
        if graph.unresolved_workspace:
            # Units were seen and the edge count is 0 — the exact fingerprint of `independence`,
            # and here it would be a lie. Something named a workspace package of this repo and
            # did not resolve, so at least those edges exist and went unmeasured. Reporting the
            # strongest rule in the catalogue off a graph with known holes in it is precisely the
            # vacuous gate D5 exists to catch.
            unresolved = sorted(graph.unresolved_workspace)
            return {
                "status": "refused",
                "reason": (
                    f"{len(unresolved)} specifier(s) name a package this repo's `workspaces` "
                    "declares, and none of them resolved to a file — so cross-package edges "
                    "exist and were not measured. 0 edges here means the resolver failed, not "
                    "that the units are independent. Fix the resolution (an `exports` map this "
                    "reader does not understand, or a tsconfig `paths`-only monorepo) before "
                    "anything is proposed."
                ),
                "units": sorted(graph.units_seen),
                "unresolved_workspace_imports": unresolved,
                "candidates": [],
            }
        return {
            "status": "proposed",
            "units": sorted(graph.units_seen),
            "total_edges": 0,
            "cycles": [],
            # Every unit with an empty list: "imports nothing" is the claim, and it is the
            # strongest allow-list there is. Omitting it here made the renderer refuse a repo
            # whose boundaries were the cleanest of the five.
            "allow_list": {unit: [] for unit in sorted(graph.units_seen)},
            "test_only_edges": [],
            "candidates": [
                {
                    "kind": "independence",
                    "forbid": "any import between units",
                    "evidence": (
                        f"the scan reached {len(graph.units_seen)} units and measured 0 imports "
                        "between them. Mutual independence is a stronger property than any "
                        "direction rule, and it is the one most easily lost by accident"
                    ),
                }
            ],
            "not_proposed": [],
        }

    cycles = find_cycles(graph)
    candidates = [
        {"kind": c.kind, "forbid": f"{c.source} -> {c.target}", "evidence": c.evidence}
        for c in one_way_candidates(graph)
    ] + [
        {"kind": c.kind, "forbid": f"{c.source} <-> {c.target}", "evidence": c.evidence}
        for c in independent_pairs(graph)
    ]
    if not cycles:
        candidates.insert(
            0,
            {
                "kind": "no-circular",
                "forbid": "any cycle",
                "evidence": (
                    f"0 cycles across {len(graph.units)} units and {graph.total_edges} edges "
                    "measured now. The rule keeps it that way without anyone remembering to look"
                ),
            },
        )

    return {
        "status": "proposed",
        "units": sorted(graph.units),
        "total_edges": graph.total_edges,
        "cycles": [list(c) for c in cycles],
        "modules": sorted(graph.modules),
        "allow_list": allow_list(graph),
        # Edges that live only in test files, reported apart and never folded into the allow-list.
        # Whoever adopts the config decides: exclude tests from the linter so the enforced set
        # matches the measured one, or widen a rule deliberately. Silently widening is the one
        # option this tool will not take for you.
        "test_only_edges": sorted(
            f"{source} -> {target}"
            for (source, target), _ in graph.test_edges.items()
            if target not in allow_list(graph).get(source, [])
        ),
        "candidates": candidates,
        "not_proposed": (
            []
            if not cycles
            else [
                {
                    "kind": "no-circular",
                    "reason": (
                        f"{len(cycles)} cycle(s) exist today, so this rule would fail on its first "
                        "run. That is a finding for the backlog, not a gate to adopt"
                    ),
                }
            ]
        ),
    }


# ---------------------------------------------------------------------------
# Graph construction, per language
# ---------------------------------------------------------------------------


def go_graph(manifest_dir: Path) -> Graph:
    """Build the unit graph from `go list -json ./...`, across every module of the repo.

    The Go toolchain resolves imports exactly, so this needs no third-party parser and cannot
    silently under-read the way a regex scan can.

    A `go.work` repo needs each module walked separately: `go list ./...` at a workspace root
    exits with *"directory prefix . does not contain modules listed in go.work"* and returns
    nothing. `theo` is exactly this shape — eight modules, no `go.mod` at the root — and the first
    version of this tool refused it, which meant the largest Go repo in the ecosystem was the one
    it could not read.

    Modules OUTSIDE the repo are dropped. `theo`'s workspace uses `../theo-contracts`, a sibling
    repository with its own boundaries; folding its packages in here would produce rules for
    `theo` about code `theo` does not own.
    """
    graph = Graph()
    modules = _workspace_modules(manifest_dir)
    if modules:
        for module_dir in modules:
            rel = module_dir.relative_to(manifest_dir.resolve()).as_posix()
            graph.modules.append(rel)
            _add_module(graph, module_dir, prefix=rel)
        return graph
    _add_module(graph, manifest_dir, prefix="")
    return graph


def _workspace_modules(repo: Path) -> list[Path]:
    """Module directories a `go.work` declares, restricted to those inside the repo."""
    if not (repo / "go.work").is_file():
        return []
    try:
        result = subprocess.run(
            ["go", "list", "-m", "-json"],
            cwd=str(repo),
            capture_output=True,
            text=True,
            timeout=_GO_LIST_TIMEOUT_SEC,
            check=False,
        )
    except (FileNotFoundError, subprocess.SubprocessError, OSError):
        return []
    if result.returncode != 0:
        return []

    inside: list[Path] = []
    for module in _iter_json_objects(result.stdout):
        directory = str(module.get("Dir", ""))
        if not directory:
            continue
        path = Path(directory).resolve()
        try:
            path.relative_to(repo.resolve())
        except ValueError:
            continue  # sibling repo pulled in by `use ../x`
        inside.append(path)
    return inside


def _add_module(graph: Graph, manifest_dir: Path, *, prefix: str) -> None:
    """Fold one module's package graph into `graph`, namespaced by its directory.

    The prefix matters in a workspace: `api/internal/auth` and `pkg/internal/auth` are different
    units, and merging them by their in-module path would invent edges between modules that never
    import each other.
    """
    joined = (lambda unit: f"{prefix}/{unit}" if prefix and unit else unit)
    try:
        result = subprocess.run(
            ["go", "list", "-json", "./..."],
            cwd=str(manifest_dir),
            capture_output=True,
            text=True,
            timeout=_GO_LIST_TIMEOUT_SEC,
            check=False,
        )
    except (FileNotFoundError, subprocess.SubprocessError, OSError):
        return
    if result.returncode != 0 or not result.stdout.strip():
        return

    module = ""
    packages = list(_iter_json_objects(result.stdout))
    for pkg in packages:
        module = module or str((pkg.get("Module") or {}).get("Path", ""))
    if not module:
        return

    # Every package path the module actually ships, relative to the module root. This is what makes
    # "smallest prefix that is itself a package" answerable without probing the filesystem.
    own = frozenset(
        str(pkg.get("ImportPath", ""))[len(module) :].lstrip("/")
        for pkg in packages
        if str(pkg.get("ImportPath", "")).startswith(module)
    ) - {""}

    for pkg in packages:
        source = joined(_unit_of_import(str(pkg.get("ImportPath", "")), module, own))
        graph.see(source)
        # `Imports` is production only. Test imports are measured apart, in `test_edges`, because
        # folding them in here would WIDEN the production allow-list: a test crossing a boundary
        # would license production to cross it too, which is the softening this tool exists to
        # refuse. Measured on theo-cloud: 49 of the violations against a production-derived
        # allow-list came from `_test.go` files alone.
        for imported in pkg.get("Imports") or []:
            target = joined(_unit_of_import(str(imported), module, own))
            if target:
                graph.add(source, target)
        for key in ("TestImports", "XTestImports"):
            for imported in pkg.get(key) or []:
                target = joined(_unit_of_import(str(imported), module, own))
                if target and target != source:
                    graph.test_edges[(source, target)] = graph.test_edges.get((source, target), 0) + 1


def typescript_graph(manifest_dir: Path) -> Graph:
    """Build the unit graph from the repo's own source, via the shared tree-sitter extractor.

    Relative imports count, and so do bare specifiers that name a package this repo's own
    `workspaces` field declares — in a monorepo those ARE the cross-unit edges. Every other bare
    specifier is somebody else's code; treating `react` as a unit would invent architecture out
    of the dependency list.
    """
    from scripts.check_symbol_fab import extract_imports_and_calls  # noqa: PLC0415

    graph = Graph()
    workspace = _workspace_packages(manifest_dir)
    sources = _ts_sources(manifest_dir)
    # The TypeScript analogue of "smallest prefix that is itself a package": a directory holding
    # source files directly implements; one holding only subdirectories groups. Without this, TS
    # kept the coarse first-segment granularity that measurement rejected on the Go side, where
    # collapsing `internal/`'s 28 subpackages into one unit hid every dependency between them.
    owning = _ts_owning_dirs(sources, manifest_dir)
    for path in sorted(manifest_dir.rglob("*")):
        if path.suffix not in {".ts", ".tsx", ".mts"} or not path.is_file():
            continue
        if any(part in _NOT_A_UNIT for part in path.parts) or ".test." in path.name:
            continue
        source = _unit_of_path(path, manifest_dir, owning)
        if not source:
            continue
        graph.see(source)
        static = [getattr(symbol, "module", "") or "" for symbol in extract_imports_and_calls(path, "typescript")]
        for module in static + _dynamic_imports(path):
            if module.startswith("."):
                resolved = (path.parent / module).resolve()
            else:
                resolved = _workspace_import(module, workspace, graph)
            if resolved is None:
                continue
            try:
                target = _unit_of_path(resolved, manifest_dir.resolve(), owning)
            except ValueError:
                continue
            if target:
                graph.add(source, target)
    return graph


#: `await import('...')`. The shared extractor reads import STATEMENTS; a dynamic import is a call
#: expression and it does not see one. That is fine for D2, whose question is whether a symbol was
#: fabricated, and wrong here, where the question is whether an edge exists. Measured on TheoCode:
#: 17 of `cli -> agent`'s 23 crossings and 3 of `tui -> agent`'s are dynamic, so the static count
#: alone under-reports by 20. Direction survived there — every dynamic import ran the same way as
#: a static one — but a repo where a reverse edge exists ONLY dynamically would get a one-way rule
#: proposed against it and go red on its first run, against this tool's whole criterion.
#:
#: A regex rather than a second parser pass: the target is a literal string inside a known call
#: shape, and the failure mode is missing a match (under-reading, the safe direction), never
#: inventing one. A computed specifier — `import(someVar)` — is unreadable by any static means and
#: is missed by both.
_DYNAMIC_IMPORT_RE = re.compile(r"""\bimport\s*\(\s*['"]([^'"]+)['"]""")


def _dynamic_imports(path: Path) -> list[str]:
    """Specifiers reached via `import('...')`, which the statement-level extractor cannot see."""
    try:
        return _DYNAMIC_IMPORT_RE.findall(path.read_text(encoding="utf-8", errors="replace"))
    except OSError:
        return []


def _workspace_import(module: str, workspace: dict[str, Path], graph: Graph) -> Path | None:
    """Resolve a bare specifier IF it names one of this repo's own workspace packages.

    A specifier that names a workspace package and does not resolve is recorded rather than
    dropped. That is the difference between "these units exchange nothing" and "we could not
    read what they exchange", and `propose` is only allowed to conclude the first.
    """
    if not workspace:
        return None
    # `@scope/name/sub/path` — the package name is the longest declared prefix, since a scoped
    # name contains a `/` itself and splitting on the first one would never match.
    name = next((n for n in workspace if module == n or module.startswith(f"{n}/")), None)
    if name is None:
        return None
    target = _export_target(workspace[name], module[len(name) :].lstrip("/"))
    if target is None:
        graph.unresolved_workspace.add(module)
    return target


def _iter_json_objects(stream: str):
    """`go list -json` emits concatenated objects, not an array."""
    decoder = json.JSONDecoder()
    index = 0
    text = stream.strip()
    while index < len(text):
        try:
            obj, offset = decoder.raw_decode(text, index)
        except json.JSONDecodeError:
            return
        yield obj
        index = offset
        while index < len(text) and text[index] in " \n\r\t":
            index += 1


def _unit_of_import(import_path: str, module: str, packages: frozenset[str] = frozenset()) -> str:
    """The smallest prefix of the path that is ITSELF a package. External imports are not units.

    Taking the first segment was wrong, and measurably so. In theo-cloud, `internal/` holds 0 Go
    files of its own and 28 subdirectories: it groups, it does not implement. Collapsing all 28
    into one unit hid every dependency between them — `internal/auth -> internal/account` became
    an invisible self-import — and left 44 packages matched by no component at all. The rules
    governed 2 units of a 35-package module and reported success.

    A directory with no sources of its own is a container. Descending past it is not a matter of
    taste: `internal` is absent from the package list precisely because there is nothing there to
    govern, and `internal/auth` is present because there is.

    Falls back to the first segment when the package list is unavailable, which keeps the function
    usable in tests that do not build one.
    """
    if not import_path.startswith(module):
        return ""
    rest = import_path[len(module) :].lstrip("/")
    if not rest:
        # The package AT the module root — `main.go`, `tools.go`. Returning "" left it governed by
        # no component at all: measured on theo, 22 files across three modules sat outside every
        # rule while the config reported full coverage. `.` is what go-arch-lint accepts for it.
        return "."
    # Every segment, not just the first. `dashboard/node_modules/flatted/golang/pkg/flatted` is a
    # Go file vendored inside a TypeScript app's node_modules; checking only the head let it
    # through and it became an architectural unit of theo-cloud.
    if any(segment in _NOT_A_GO_UNIT for segment in rest.split("/")):
        return ""
    if not packages:
        return rest.split("/", 1)[0]
    segments = rest.split("/")
    for depth in range(1, len(segments) + 1):
        prefix = "/".join(segments[:depth])
        if prefix in packages:
            return prefix
    return rest


def _ts_sources(root: Path) -> list[Path]:
    """Every TypeScript source under `root` that is not test, fixture or vendored."""
    out: list[Path] = []
    for path in root.rglob("*"):
        if path.suffix not in {".ts", ".tsx", ".mts"} or not path.is_file():
            continue
        if any(part in _NOT_A_UNIT for part in path.parts) or ".test." in path.name:
            continue
        out.append(path)
    return out


def _workspace_packages(root: Path) -> dict[str, Path]:
    """`name -> package directory`, for every package the root `package.json` declares.

    A monorepo's cross-package imports are BARE specifiers — `@theocode/agent`, not `../agent`.
    Skipping them because "a bare specifier is a package" was true and catastrophic: on TheoCode
    it measured 0 edges across 4 units that exchange 80 imports, and then proposed `independence`
    — a rule forbidding every one of them. Adopting it would have been red on arrival, which is
    the one thing this tool exists to make impossible.

    `workspaces` is the declaration that separates `@theocode/agent` (a unit of this repo) from
    `react` (somebody else's code). Without it there is nothing to distinguish them by, and
    guessing from the `@scope/` prefix would invent architecture out of a naming convention.
    """
    manifest = root / "package.json"
    if not manifest.is_file():
        return {}
    try:
        declared = json.loads(manifest.read_text(encoding="utf-8")).get("workspaces") or []
    except (json.JSONDecodeError, OSError):
        return {}
    # npm accepts both `["packages/*"]` and `{"packages": ["packages/*"]}`.
    if isinstance(declared, dict):
        declared = declared.get("packages") or []

    out: dict[str, Path] = {}
    for pattern in declared:
        if not isinstance(pattern, str):
            continue
        for candidate in sorted(root.glob(pattern)):
            if not candidate.is_dir() or any(part in _NOT_A_UNIT for part in candidate.parts):
                continue
            try:
                name = json.loads((candidate / "package.json").read_text(encoding="utf-8")).get("name")
            except (json.JSONDecodeError, OSError):
                continue
            if isinstance(name, str) and name:
                out[name] = candidate
    return out


def _export_target(package_dir: Path, subpath: str) -> Path | None:
    """The file a workspace package's `exports` map points a subpath at.

    `exports` is the resolution contract — it is what makes `@theocode/shared/shutdown` mean
    `packages/shared/src/shutdown.ts` at runtime. Reading it beats guessing a layout: `shared`
    declares three subpath exports and NO `.` entry, so any convention like "the unit is
    `<pkg>/src/index.ts`" would have resolved nothing for it.
    """
    try:
        manifest = json.loads((package_dir / "package.json").read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return None

    exports = manifest.get("exports")
    key = "." if not subpath else f"./{subpath}"
    target = None
    if isinstance(exports, dict):
        target = exports.get(key)
        # Conditional exports: {"./x": {"import": "./src/x.ts"}}. Take the first string leaf —
        # every condition of one subpath points into the same file tree, so they agree on a unit.
        while isinstance(target, dict):
            target = next((v for v in target.values() if isinstance(v, (str, dict))), None)
    elif isinstance(exports, str) and key == ".":
        target = exports
    if not isinstance(target, str):
        target = manifest.get("main") if key == "." else None
    if not isinstance(target, str):
        return None
    return (package_dir / target).resolve()


def _ts_owning_dirs(sources: list[Path], root: Path) -> frozenset[str]:
    """Directories that hold source files DIRECTLY — the ones that implement rather than group."""
    dirs: set[str] = set()
    for path in sources:
        try:
            rel = path.relative_to(root).parent
        except ValueError:
            continue
        if rel != Path("."):
            dirs.add(rel.as_posix())
    return frozenset(dirs)


def _unit_of_path(path: Path, root: Path, owning: frozenset[str] = frozenset()) -> str:
    """Smallest prefix directory that holds sources of its own. Falls back to the first segment."""
    try:
        rel = path.relative_to(root)
    except ValueError:
        return ""
    parts = rel.parts[:-1]  # drop the filename
    if not parts:
        return ""
    if any(part in _NOT_A_UNIT for part in parts):
        return ""
    if not owning:
        return parts[0]
    for depth in range(1, len(parts) + 1):
        prefix = "/".join(parts[:depth])
        if prefix in owning:
            return prefix
    return "/".join(parts)


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if not args:
        print("usage: propose_rules.py <repo-path> [--language go|typescript]", file=sys.stderr)
        return 2
    repo = Path(args[0]).resolve()
    language = args[args.index("--language") + 1] if "--language" in args else _detect(repo)
    if language is None:
        print(json.dumps({"status": "refused", "reason": "no supported manifest at the repo root"}))
        return 0

    graph = {"go": go_graph, "typescript": typescript_graph}[language](repo)
    result = propose(graph)
    result["language"] = language
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0


def _detect(repo: Path) -> str | None:
    if (repo / "go.mod").is_file() or (repo / "go.work").is_file():
        return "go"
    if (repo / "package.json").is_file():
        return "typescript"
    return None


if __name__ == "__main__":
    raise SystemExit(main())
