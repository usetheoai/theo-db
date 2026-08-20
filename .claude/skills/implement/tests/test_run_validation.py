"""Tests for run_validation.py — verifies graceful pre-code SKIP + integration."""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


SCRIPT = Path(__file__).parent.parent / "scripts" / "run_validation.py"

from run_validation import wiring_summary  # noqa: E402 — conftest puts scripts/ on sys.path


def _git(repo: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(repo), *args],
                          capture_output=True, text=True, check=True).stdout


def _init_repo(tmp_path: Path) -> Path:
    repo = tmp_path / "repo"
    (repo / "src").mkdir(parents=True)
    _git(repo, "init", "-q")
    _git(repo, "config", "user.email", "t@t.t")
    _git(repo, "config", "user.name", "t")
    return repo


def _commit(repo: Path, rel: str, content: str, msg: str = "feat") -> str:
    path = repo / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    _git(repo, "add", rel)
    _git(repo, "commit", "-q", "-m", msg)
    return _git(repo, "rev-parse", "HEAD").strip()


def _write_progress(project_root: Path, tasks: list[dict], slug: str = "wsg") -> None:
    impl_dir = project_root / ".claude" / "knowledge-base" / "implementations"
    impl_dir.mkdir(parents=True, exist_ok=True)
    (impl_dir / f".progress-{slug}.json").write_text(
        json.dumps({"slug": slug, "tasks": tasks}), encoding="utf-8"
    )


def test_wiring_summary_detects_fabricated_evidence(tmp_path: Path) -> None:
    """GAP 3: self-reported pillar (a) pass + an actually-uncalled symbol = fabrication.

    The final gate must NOT trust the progress file: it re-derives symbols from the
    committed diff and re-runs check_wiring. A dishonest `wiring.a=pass` over an
    orphan symbol is caught as fabricated evidence, status FAIL.
    """
    repo = _init_repo(tmp_path)
    sha = _commit(repo, "src/orphan.py", "def orphan_fn(x):\n    return x\n")
    _write_progress(repo, [
        {"id": "T1.1", "phase": "1", "commit_sha": sha, "wiring": {"a": "pass"}},
    ])
    result = wiring_summary(repo, "wsg")
    assert result["status"] == "FAIL"
    assert result["fabricated_wiring_evidence"] is True
    assert "orphan_fn" in result["pillar_a_fail_symbols"]


def test_wiring_summary_passes_when_recheck_confirms_caller(tmp_path: Path) -> None:
    """A genuinely-wired symbol (real production caller) passes the independent recheck."""
    repo = _init_repo(tmp_path)
    sha = _commit(repo, "src/order.py", "def compute_total(x):\n    return x\n")
    _commit(repo, "src/app.py", "from order import compute_total\nprint(compute_total(1))\n")
    _write_progress(repo, [
        {"id": "T1.1", "phase": "1", "commit_sha": sha, "wiring": {"a": "pass"}},
    ])
    result = wiring_summary(repo, "wsg")
    assert result["status"] == "PASS"
    assert result["pillar_a_fails"] == 0


def test_wiring_summary_na_when_nothing_verifiable(tmp_path: Path) -> None:
    """No SHAs / no git → cannot re-verify → N/A, NOT a PASS laundered from a claim."""
    _write_progress(tmp_path, [
        {"id": "T1.1", "phase": "1", "wiring": {"a": "pass"}},  # no commit_sha
    ])
    result = wiring_summary(tmp_path, "wsg")
    assert result["status"] == "N/A"
    assert result["symbols_resolved"] == 0
    # The claim is preserved for audit but did NOT produce a PASS.
    assert result["self_reported_pillar_a_pass"] == 1


def _run_validation(slug: str, project_root: Path) -> tuple[int, dict]:
    result = subprocess.run(
        [sys.executable, str(SCRIPT), slug, "--project-root", str(project_root), "--no-write-report"],
        capture_output=True,
        text=True,
    )
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        data = {"raw": result.stdout, "stderr": result.stderr}
    return result.returncode, data


def test_pre_code_phase_all_skip(fake_project: Path) -> None:
    """No package.json → all npm-based gates SKIP gracefully; overall=PARTIAL."""
    rc, data = _run_validation("test-slug", fake_project)
    assert rc == 0  # PARTIAL is exit 0 (no failures, just skips)
    assert data["overall_status"] == "PARTIAL"
    skips = [c for c in data["checks"] if c.get("status") == "SKIP"]
    assert len(skips) >= 4


def test_with_package_json_and_passing_scripts(fake_project: Path) -> None:
    """Package.json with test/typecheck/lint that exit 0 → all PASS (or some SKIP)."""
    (fake_project / "package.json").write_text(
        json.dumps({
            "name": "fake",
            "scripts": {
                "test": "true",  # exit 0
                "typecheck": "true",
                "lint": "true",
            }
        }),
        encoding="utf-8",
    )
    rc, data = _run_validation("test-slug", fake_project)
    # No FAILs expected; PASS or SKIP only
    fails = [c for c in data["checks"] if c.get("status") == "FAIL"]
    assert len(fails) == 0


def test_with_failing_test_script(fake_project: Path) -> None:
    """Package.json with `test` that exits 1 → npm test FAIL → overall=FAIL."""
    (fake_project / "package.json").write_text(
        json.dumps({
            "name": "fake",
            "scripts": {
                "test": "false",  # exit 1
            }
        }),
        encoding="utf-8",
    )
    rc, data = _run_validation("test-slug", fake_project)
    assert rc == 1
    assert data["overall_status"] == "FAIL"
    test_check = next(c for c in data["checks"] if c.get("name") == "npm test")
    assert test_check["status"] == "FAIL"


def test_new_gates_are_wired_into_validation(fake_project: Path) -> None:
    """GAP 1+2 / GAP 6: the acceptance-criteria and test-obligation gates must run as
    part of the final validation, not exist as orphan scripts."""
    plan_dir = fake_project / ".claude" / "knowledge-base" / "plans"
    plan_dir.mkdir(parents=True, exist_ok=True)
    (plan_dir / "test-slug-plan.md").write_text(
        "# Plan\n\n### T1.1 — X\n\n#### Acceptance Criteria\n"
        "- [ ] Backward compatibility preserved across public API\n",
        encoding="utf-8",
    )
    _, data = _run_validation("test-slug", fake_project)
    names = [c["name"] for c in data["checks"]]
    assert "acceptance_criteria" in names
    assert "test_obligations" in names
    ac = next(c for c in data["checks"] if c["name"] == "acceptance_criteria")
    assert ac["status"] != "SKIP"  # plan found → criteria actually audited


def test_checkpoint_consistency_gate_catches_unrecorded_task(tmp_path: Path) -> None:
    """End-to-end: a task committed in git but missing from the checkpoint fails the
    checkpoint_consistency gate inside run_validation."""
    repo = _init_repo(tmp_path)
    sha1 = _commit(repo, "src/a.py", "x = 1\n", "feat: a\n\nT1.1: foo")
    _commit(repo, "src/b.py", "y = 2\n", "feat: b\n\nT1.2: bar")  # committed, but not in checkpoint
    plan_dir = repo / ".claude" / "knowledge-base" / "plans"
    plan_dir.mkdir(parents=True, exist_ok=True)
    (plan_dir / "ck-plan.md").write_text(
        "## Phase 1\n### T1.1 — Foo\nbody\n### T1.2 — Bar\nbody\n", encoding="utf-8")
    _write_progress(repo, [
        {"id": "T1.1", "phase": "1", "status": "committed", "commit_sha": sha1},
    ], slug="ck")

    rc, data = _run_validation("ck", repo)
    cc = next(c for c in data["checks"] if c["name"] == "checkpoint_consistency")
    assert cc["status"] == "FAIL"
    assert "task_committed_in_git_not_in_progress" in [f["code"] for f in cc["findings"]]
    assert rc == 1


def test_malformed_checkpoint_fails_validation(fake_project: Path) -> None:
    """The progress-schema gate must catch a malformed checkpoint (the prompt's old
    bare-object shape) and FAIL the whole validation, not let gates degrade silently."""
    impl = fake_project / ".claude" / "knowledge-base" / "implementations"
    impl.mkdir(parents=True, exist_ok=True)
    (impl / ".progress-test-slug.json").write_text(
        json.dumps({"task_id": "T1.1", "status": "committed"}),  # no 'tasks' envelope
        encoding="utf-8",
    )
    rc, data = _run_validation("test-slug", fake_project)
    ps = next(c for c in data["checks"] if c["name"] == "progress_schema")
    assert ps["status"] == "FAIL"
    assert "progress_missing_tasks" in [f["code"] for f in ps["findings"]]
    assert rc == 1
    assert data["overall_status"] == "FAIL"


def test_summary_buckets_account_for_every_check(fake_project: Path) -> None:
    """Regression: pass+fail+skip+warn+partial+n_a must equal total — WARN and
    PARTIAL statuses (from the code-quality gate) used to be dropped from the summary."""
    _, data = _run_validation("test-slug", fake_project)
    s = data["summary"]
    for bucket in ("pass", "fail", "skip", "warn", "partial", "n_a"):
        assert bucket in s, f"summary missing bucket '{bucket}'"
    assert s["pass"] + s["fail"] + s["skip"] + s["warn"] + s["partial"] + s["n_a"] == s["total"]


# T2.1 — patterns-consumption advisory (patterns-consumption-gate-plan, ADR D3)

from run_validation import check_patterns_advisory  # noqa: E402


def test_patterns_advisory_never_fails(tmp_path: Path) -> None:
    plans = tmp_path / ".claude" / "knowledge-base" / "plans"
    plans.mkdir(parents=True)
    (plans / "demo-plan.md").write_text(
        "# Plan: demo\n## Prior Art & Related Work\n- Patterns skills: `foo-patterns` Pattern P1.\n"
    )
    src = tmp_path / "src"
    src.mkdir()
    (src / "impl.py").write_text("print('no skill mention here')\n")
    impl = tmp_path / ".claude" / "knowledge-base" / "implementations"
    impl.mkdir(parents=True)
    (impl / ".progress-demo.json").write_text(json.dumps({
        "slug": "demo",
        "tasks": [{"id": "T1.1", "phase": "1", "status": "committed", "files": ["src/impl.py"]}],
    }))
    r = check_patterns_advisory(tmp_path, "demo")
    assert r["status"] == "WARN"           # advisory, surfaced
    assert r["status"] != "FAIL"           # never blocks handoff (ADR D3)
    assert "foo-patterns" in r["not_found"]


def test_patterns_advisory_absent_when_no_citation(tmp_path: Path) -> None:
    plans = tmp_path / ".claude" / "knowledge-base" / "plans"
    plans.mkdir(parents=True)
    (plans / "demo-plan.md").write_text("# Plan: demo\n## Goal\nNothing special here.\n")
    r = check_patterns_advisory(tmp_path, "demo")
    assert r["status"] == "N/A"


def _standalone_project(tmp_path: Path, *, tasks: list[dict]) -> Path:
    """A project in the STANDALONE layout — knowledge-base at the root, no `.claude/` wrapper.

    `rules/knowledge-base-location.md` makes this canonical for the kit's own repository, which
    is exactly where the kit dogfoods itself.
    """
    (tmp_path / "knowledge-base" / "plans").mkdir(parents=True)
    (tmp_path / "knowledge-base" / "implementations").mkdir(parents=True)
    (tmp_path / "knowledge-base" / "plans" / "s-plan.md").write_text(
        "## Phase 1 — core\n\n### T1.1 — first\n### T1.2 — skipped\n", encoding="utf-8"
    )
    (tmp_path / "knowledge-base" / "implementations" / ".progress-s.json").write_text(
        json.dumps({"tasks": tasks}), encoding="utf-8"
    )
    subprocess.run(["git", "init", "-q", str(tmp_path)], check=True)
    return tmp_path


def test_find_progress_reads_the_standalone_layout(tmp_path: Path) -> None:
    """Three call sites hardcoded `.claude/`, while `_find_plan` beside them handled both.

    In the standalone layout every one of them answered SKIP — "no progress checkpoint,
    implement may not have run" — for a checkpoint sitting on disk. A gate that skips because
    it looked in the wrong directory is indistinguishable in the report from one that
    legitimately had nothing to check, which is why it survived.
    """
    from run_validation import _find_progress

    root = _standalone_project(tmp_path, tasks=[{"id": "T1.1", "phase": 1, "status": "committed"}])
    found = _find_progress(root, "s")
    assert found is not None
    assert found == root / "knowledge-base" / "implementations" / ".progress-s.json"


def test_find_progress_still_prefers_the_plugin_layout(tmp_path: Path) -> None:
    """The plugin layout is canonical for every consumer; standalone is the single exception."""
    from run_validation import _find_progress

    root = _standalone_project(tmp_path, tasks=[])
    plugin = root / ".claude" / "knowledge-base" / "implementations"
    plugin.mkdir(parents=True)
    (plugin / ".progress-s.json").write_text(json.dumps({"tasks": []}), encoding="utf-8")
    assert _find_progress(root, "s") == plugin / ".progress-s.json"


def test_checkpoint_gate_catches_a_skipped_task_in_the_standalone_layout(tmp_path: Path) -> None:
    """End-to-end: the two defects compounded — the gate could not find the checkpoint, and
    even when it did it could not see an omitted task."""
    from run_validation import check_checkpoint_consistency_gate

    root = _standalone_project(tmp_path, tasks=[{"id": "T1.1", "phase": 1, "status": "committed"}])
    result = check_checkpoint_consistency_gate(root, "s")
    assert result["status"] == "FAIL"
    assert [f["code"] for f in result["findings"] if f.get("severity") != "INFO"] == ["plan_task_absent_from_progress"]


# ---------------------------------------------------------------------------
# Test-execution gate (multi-language). The npm-only checks answered SKIP on a
# Python/Go/Rust repo, overall became PARTIAL and PARTIAL exits 0 — so
# VALIDATION_GATE_PASSED could be emitted without a single test having run.
# ---------------------------------------------------------------------------

def _check(data: dict, name: str) -> dict:
    return next(c for c in data["checks"] if c.get("name") == name)


def test_python_manifest_with_passing_tests_runs_the_suite(fake_project: Path) -> None:
    """A Python project's tests actually execute — not SKIP for lack of package.json."""
    (fake_project / "pyproject.toml").write_text("[project]\nname='fake'\n", encoding="utf-8")
    (fake_project / "tests" / "test_ok.py").write_text(
        "def test_ok():\n    assert True\n", encoding="utf-8"
    )
    rc, data = _run_validation("test-slug", fake_project)
    suite = _check(data, "python tests")
    assert suite["status"] == "PASS", suite
    assert _check(data, "test_execution")["status"] == "PASS"


def test_python_failing_tests_fail_the_validation(fake_project: Path) -> None:
    """A red Python suite blocks the gate exactly like a red npm suite does."""
    (fake_project / "pyproject.toml").write_text("[project]\nname='fake'\n", encoding="utf-8")
    (fake_project / "tests" / "test_red.py").write_text(
        "def test_red():\n    assert False\n", encoding="utf-8"
    )
    rc, data = _run_validation("test-slug", fake_project)
    assert rc == 1
    assert data["overall_status"] == "FAIL"
    assert _check(data, "python tests")["status"] == "FAIL"


def test_manifest_present_but_no_suite_ran_is_a_fail(fake_project: Path) -> None:
    """The load-bearing case: a language manifest exists and nothing executed.

    SKIP here is indistinguishable from 'legitimately nothing to check', which is
    how a green validation could mean no test ever ran. It must FAIL instead.
    """
    (fake_project / "pyproject.toml").write_text("[project]\nname='fake'\n", encoding="utf-8")
    rc, data = _run_validation("test-slug", fake_project)
    assert rc == 1
    gate = _check(data, "test_execution")
    assert gate["status"] == "FAIL"
    assert "python" in gate["languages_detected"]


def test_package_json_without_test_script_is_a_fail(fake_project: Path) -> None:
    """A JS project that cannot run tests at all is not a pass."""
    (fake_project / "package.json").write_text(
        json.dumps({"name": "fake", "scripts": {"lint": "true"}}), encoding="utf-8"
    )
    rc, data = _run_validation("test-slug", fake_project)
    assert rc == 1
    assert _check(data, "test_execution")["status"] == "FAIL"


def test_no_manifest_at_all_still_skips_gracefully(fake_project: Path) -> None:
    """Pre-code phase is a legitimate SKIP — the gate must not punish an empty repo."""
    rc, data = _run_validation("test-slug", fake_project)
    gate = _check(data, "test_execution")
    assert gate["status"] == "SKIP"
    assert rc == 0


# ---------------------------------------------------------------------------
# Coverage gate. It used to run `npm run test:coverage` and call exit 0 a PASS
# without ever reading a coverage report — a gate named after a number it never
# looked at.
# ---------------------------------------------------------------------------

def _coverage_project(root: Path, script: str = "true") -> None:
    (root / "package.json").write_text(
        json.dumps({"name": "fake", "scripts": {"test:coverage": script}}), encoding="utf-8"
    )


def test_coverage_reads_the_json_summary_and_passes_above_threshold(fake_project: Path) -> None:
    _coverage_project(fake_project)
    summary = fake_project / "coverage" / "coverage-summary.json"
    summary.parent.mkdir(parents=True, exist_ok=True)
    summary.write_text(json.dumps({"total": {"lines": {"pct": 95.5}}}), encoding="utf-8")
    rc, data = _run_validation("test-slug", fake_project)
    check = _check(data, "coverage")
    assert check["status"] == "PASS"
    assert check["coverage_pct"] == 95.5


def test_coverage_below_threshold_fails(fake_project: Path) -> None:
    """The whole point of the gate: a measured number under the floor blocks."""
    _coverage_project(fake_project)
    summary = fake_project / "coverage" / "coverage-summary.json"
    summary.parent.mkdir(parents=True, exist_ok=True)
    summary.write_text(json.dumps({"total": {"lines": {"pct": 41.0}}}), encoding="utf-8")
    rc, data = _run_validation("test-slug", fake_project)
    assert rc == 1
    check = _check(data, "coverage")
    assert check["status"] == "FAIL"
    assert check["coverage_pct"] == 41.0


def test_coverage_without_a_parseable_report_is_not_a_pass(fake_project: Path) -> None:
    """Exit 0 with no report means the threshold was never verified — WARN, not PASS."""
    _coverage_project(fake_project)
    rc, data = _run_validation("test-slug", fake_project)
    check = _check(data, "coverage")
    assert check["status"] == "WARN"
    assert "not verified" in check["reason"].lower()


def test_coverage_reads_cobertura_xml(fake_project: Path) -> None:
    """coverage.py / Cobertura XML is the Python-side artifact."""
    _coverage_project(fake_project)
    (fake_project / "coverage.xml").write_text(
        '<?xml version="1.0" ?><coverage line-rate="0.873"></coverage>', encoding="utf-8"
    )
    rc, data = _run_validation("test-slug", fake_project)
    check = _check(data, "coverage")
    assert check["status"] == "PASS"
    assert check["coverage_pct"] == 87.3


def test_coverage_threshold_comes_from_the_project_rules_file(fake_project: Path) -> None:
    """A project may raise the floor; the report says where the number came from."""
    _coverage_project(fake_project)
    rules_dir = fake_project / "rules"
    rules_dir.mkdir(parents=True, exist_ok=True)
    (rules_dir / "code-quality-thresholds.txt").write_text(
        "coverage.min_percent = 90\n", encoding="utf-8"
    )
    summary = fake_project / "coverage" / "coverage-summary.json"
    summary.parent.mkdir(parents=True, exist_ok=True)
    summary.write_text(json.dumps({"total": {"lines": {"pct": 85.0}}}), encoding="utf-8")
    rc, data = _run_validation("test-slug", fake_project)
    check = _check(data, "coverage")
    assert check["status"] == "FAIL"
    assert check["threshold"] == 90
    assert check["threshold_source"] == "project"


# ---------------------------------------------------------------------------
# Gates the agent ran on its own honour. check_tdd_shape.py and mini_review.py
# were invoked from SKILL.md prose only; the final gate never asked whether
# either had run, so skipping them left no trace.
# ---------------------------------------------------------------------------

_PHASED_PLAN = """# Plan

## Phase 1 — foundation

### T1.1 — first
#### TDD
assert add(1, 2) == 3
"""


def _write_plan(project_root: Path, slug: str, body: str) -> None:
    plans = project_root / "knowledge-base" / "plans"
    plans.mkdir(parents=True, exist_ok=True)
    (plans / f"{slug}-plan.md").write_text(body, encoding="utf-8")


def _write_standalone_progress(project_root: Path, slug: str, tasks: list[dict]) -> None:
    impl = project_root / "knowledge-base" / "implementations"
    impl.mkdir(parents=True, exist_ok=True)
    (impl / f".progress-{slug}.json").write_text(
        json.dumps({"slug": slug, "tasks": tasks}), encoding="utf-8"
    )


def test_skipped_phase_boundary_review_is_caught_by_the_final_gate(fake_project: Path) -> None:
    """A fully committed phase with no mini-review report must FAIL the validation."""
    _write_plan(fake_project, "phased", _PHASED_PLAN)
    _write_standalone_progress(fake_project, "phased", [
        {"id": "T1.1", "phase": "1", "status": "committed", "commit_sha": "abc", "files": ["src/a.py"]},
    ])
    rc, data = _run_validation("phased", fake_project)
    gate = _check(data, "phase_review")
    assert gate["status"] == "FAIL"
    assert gate["phases_closed"] == ["1"]


def test_phase_boundary_review_present_passes(fake_project: Path) -> None:
    _write_plan(fake_project, "phased", _PHASED_PLAN)
    _write_standalone_progress(fake_project, "phased", [
        {"id": "T1.1", "phase": "1", "status": "committed", "commit_sha": "abc", "files": ["src/a.py"]},
    ])
    reviews = fake_project / "knowledge-base" / "mini-reviews"
    reviews.mkdir(parents=True, exist_ok=True)
    (reviews / "phased-phase1-review-2026-08-18.md").write_text("ok", encoding="utf-8")
    rc, data = _run_validation("phased", fake_project)
    assert _check(data, "phase_review")["status"] == "PASS"


def test_plan_task_without_an_executable_tdd_shape_fails(fake_project: Path) -> None:
    """The Step 2 pre-loop gate is re-asserted at the end: a prose-only TDD block
    means the halt-loop should never have started."""
    _write_plan(fake_project, "vague", """# Plan

### T1.1 — do the thing
#### TDD
We should test that it works well.
""")
    _write_standalone_progress(fake_project, "vague", [
        {"id": "T1.1", "phase": "1", "status": "committed", "commit_sha": "abc", "files": ["src/a.py"]},
    ])
    rc, data = _run_validation("vague", fake_project)
    assert rc == 1
    gate = _check(data, "tdd_shape")
    assert gate["status"] == "FAIL"
    assert gate["tasks_without_shape"] == ["T1.1"]


def test_executable_tdd_shape_passes(fake_project: Path) -> None:
    _write_plan(fake_project, "sharp", _PHASED_PLAN)
    _write_standalone_progress(fake_project, "sharp", [
        {"id": "T1.1", "phase": "1", "status": "committed", "commit_sha": "abc", "files": ["src/a.py"]},
    ])
    rc, data = _run_validation("sharp", fake_project)
    assert _check(data, "tdd_shape")["status"] == "PASS"


def test_go_workspace_is_detected_as_go(fake_project: Path) -> None:
    """A Go workspace has `go.work` and no root `go.mod`.

    Measured on `theo` while updating its install: the repo is Go, and
    detect_languages returned [] — so test_execution would have SKIPped the
    biggest Go repo in the ecosystem. The same silence the gate exists to break,
    reintroduced by a manifest list that only knew `go.mod`.
    """
    from suite_runners import detect_languages
    (fake_project / "go.work").write_text("go 1.22\n\nuse (\n\t./svc\n)\n", encoding="utf-8")
    assert "go" in detect_languages(fake_project)


def test_go_workspace_runs_each_module_not_the_root(fake_project: Path) -> None:
    """`go test ./...` at a workspace root fails with 'directory prefix . does not
    contain modules listed in go.work' — the kit already hit this in /arch-check."""
    from suite_runners import go_workspace_modules
    (fake_project / "go.work").write_text(
        "go 1.22\n\nuse (\n\t./svc\n\t./tools\n\t../sibling-repo\n)\n", encoding="utf-8"
    )
    (fake_project / "svc").mkdir()
    (fake_project / "tools").mkdir()
    modules = go_workspace_modules(fake_project)
    assert modules == ["svc", "tools"], modules  # '../sibling-repo' is another repo's problem
