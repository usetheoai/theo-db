"""B-019 — a review that read nothing must not return a verdict.

Measured LIVE during this session's B-022 review: five reviewers wrote real findings, the
consolidator was pointed at their directory, and it returned

    "verdict": "READY_TO_MERGE", "agents_count": 0, "total_findings": 0

on a slice those five agents had independently found broken in the same place. The most permissive
verdict in the vocabulary, computed from nothing. `agents_count: 0` was right there in the JSON, and
the verdict never consulted it.

Two independent mechanisms, and fixing either alone leaves the other:

1. the glob is `*.yml`, so files with any other extension are not skipped with a warning — they are
   never seen;
2. `_read_findings_file` returns `{}` on ANY error, and the caller then counts the file in
   `agents_run` under its filename stem. A malformed file becomes an agent that "found nothing".

The second is worse: the first at least leaves an empty roster a reader might notice.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).parent.parent / "scripts" / "consolidate_findings.py"

VALID = """agent_role: architecture
findings:
  - id: F-1
    severity: LOW
    file: src/thing.ts
    line: 1
    summary: a real finding
"""

CLEAN = """agent_role: tests
findings: []
"""


def _run(findings_dir: Path, output: Path) -> tuple[int, dict[str, object]]:
    result = subprocess.run(
        [sys.executable, str(SCRIPT), "--findings-dir", str(findings_dir),
         "--output", str(output), "--slug", "fixture"],
        capture_output=True, text=True, check=False,
    )
    payload: dict[str, object] = {}
    raw = result.stdout
    if "{" in raw:
        try:
            payload = json.loads(raw[raw.index("{"):])
        except json.JSONDecodeError:
            payload = {}
    return result.returncode, payload


def test_a_directory_with_no_readable_findings_refuses_to_grade(tmp_path: Path) -> None:
    # Arrange — the B-022 shape exactly: agents wrote `.md`, the glob wants `.yml`.
    findings = tmp_path / "findings"
    findings.mkdir()
    for name in ("architecture", "tests", "wiring"):
        (findings / f"{name}.md").write_text("# real findings, wrong extension\n", encoding="utf-8")

    # Act
    code, payload = _run(findings, tmp_path / "report.md")

    # Assert — no merge-readiness verdict, and the files it could not use are NAMED. Returning
    # READY_TO_MERGE here is not a wrong grade; it is a grade of nothing.
    assert code != 0
    assert payload.get("verdict") != "READY_TO_MERGE"
    assert payload.get("verdict") == "INVALID"
    assert len(payload.get("skipped", [])) == 3


def test_a_malformed_file_is_reported_as_unreadable_not_as_an_agent(tmp_path: Path) -> None:
    # Arrange — one good agent, one file whose YAML does not parse.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "architecture.yml").write_text(VALID, encoding="utf-8")
    (findings / "tests.yml").write_text("findings: [ unclosed\n  - nope\n", encoding="utf-8")

    # Act
    code, payload = _run(findings, tmp_path / "report.md")

    # Assert — the roster is what a reader trusts, so the broken file must not be in it. Asserting
    # on the roster and not merely the finding count matters: the count was already 0 for it.
    assert code == 0
    assert payload.get("agents_run") == ["architecture"]
    assert "tests" in str(payload.get("unreadable", []))


def test_a_partial_read_grades_and_names_what_it_could_not_read(tmp_path: Path) -> None:
    # One readable agent is enough to grade — a single crashed reviewer must not block a merge — but
    # the report has to say the roster is short.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "architecture.yml").write_text(VALID, encoding="utf-8")
    (findings / "wiring.yml").write_text(": : :\n", encoding="utf-8")

    code, payload = _run(findings, tmp_path / "report.md")

    assert code == 0
    assert payload.get("total_findings") == 1
    assert payload.get("unreadable")


def test_the_yaml_spelling_is_read_not_skipped(tmp_path: Path) -> None:
    # Mutation found the `.yaml` half of the fix pinned by NOTHING: every other test in this file
    # writes `.yml`, so reverting the glob to `*.yml` only passed all of them. The original defect
    # was an extension mismatch, so leaving one spelling unread is the same defect with a different
    # letter.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "architecture.yaml").write_text(VALID, encoding="utf-8")

    code, payload = _run(findings, tmp_path / "report.md")

    assert code == 0
    assert payload.get("agents_run") == ["architecture"]
    assert payload.get("total_findings") == 1
    assert not payload.get("skipped")


def test_a_clean_review_with_two_agents_still_grades(tmp_path: Path) -> None:
    # The control. A refusal that fired on every empty finding list would satisfy the first test.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "architecture.yml").write_text(CLEAN, encoding="utf-8")
    (findings / "tests.yml").write_text(CLEAN, encoding="utf-8")

    code, payload = _run(findings, tmp_path / "report.md")

    assert code == 0
    assert payload.get("verdict") == "READY_TO_MERGE"
    assert payload.get("agents_count") == 2


# B-049 — the three findings that actually collided in the B-022 review, verbatim. They shared the
# key ("src/prompts/select-list.tsx", 217, "") and the report printed the FIRST one's LOW text under
# `## HIGH findings`, while the finding that earned the HIGH appeared nowhere.
COLLIDING = """agent_role: mixed
findings:
  - id: F-arch-3
    severity: LOW
    file: src/prompts/select-list.tsx
    line: 217
    summary: the Rule-of-3 threshold moved with this commit and nothing records it
  - id: F-dom-2
    severity: HIGH
    file: src/prompts/select-list.tsx
    line: 217
    summary: half the change is unprotected — a bare arrow mutant leaves the suite green
  - id: F-dom-6
    severity: LOW
    file: src/prompts/select-list.tsx
    line: 217
    summary: the arrow row wraps at 4 columns or fewer
"""

SAME_TEXT_TWICE = """agent_role: mixed
findings:
  - id: F-a-1
    severity: LOW
    file: src/thing.ts
    line: 10
    summary: the same defect, described identically
  - id: F-b-1
    severity: HIGH
    file: src/thing.ts
    line: 10
    summary: the same defect, described identically
"""


def test_findings_at_one_line_with_different_summaries_all_survive(tmp_path: Path) -> None:
    # A file:line is a COORDINATE, not an issue. Three findings described a refactor threshold, an
    # unprotected mutant and a wrapping bug; merging them lost two.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "mixed.yml").write_text(COLLIDING, encoding="utf-8")

    code, payload = _run(findings, tmp_path / "report.md")

    assert code == 0
    report = (tmp_path / "report.md").read_text(encoding="utf-8")
    # Assert on the TEXT, not the count: three rows could be the wrong three.
    assert "Rule-of-3 threshold" in report
    assert "unprotected" in report
    assert "wraps at 4 columns" in report
    assert payload.get("total_findings") == 3


def test_a_merged_row_keeps_the_text_that_earned_the_severity(tmp_path: Path) -> None:
    # Where merging IS right — two agents describing one defect identically — the surviving row must
    # be self-consistent: the higher severity AND the text that carries it, with both ids listed so
    # an absorbed finding is findable by name.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "mixed.yml").write_text(SAME_TEXT_TWICE, encoding="utf-8")

    code, payload = _run(findings, tmp_path / "report.md")

    assert code == 0
    assert payload.get("total_findings") == 1
    merged = payload.get("findings_by_severity", {})
    assert isinstance(merged, dict)
    report = (tmp_path / "report.md").read_text(encoding="utf-8")
    assert "F-a-1" in report and "F-b-1" in report


# Two findings that MERGE (their summaries normalise to the same string) but whose exact wording
# differs. Without this shape, "first summary wins" is indistinguishable from "the earner's summary
# wins" — my first fixture used identical text, so the mutant survived all seven tests.
MERGES_BUT_WORDED_DIFFERENTLY = """agent_role: mixed
findings:
  - id: F-low-1
    severity: LOW
    file: src/thing.ts
    line: 20
    summary: "The   Same Defect, Described"
  - id: F-high-1
    severity: HIGH
    file: src/thing.ts
    line: 20
    summary: "the same defect, described"
"""


def test_the_surviving_text_is_the_earners_own_wording(tmp_path: Path) -> None:
    # The B-049 defect in one line: the severity travelled and the text did not. These two merge —
    # same summary after whitespace/case normalisation — so the row must carry the HIGH finding's
    # id and ITS wording, not the LOW one that happened to be read first.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "mixed.yml").write_text(MERGES_BUT_WORDED_DIFFERENTLY, encoding="utf-8")

    code, payload = _run(findings, tmp_path / "report.md")

    assert code == 0
    assert payload.get("total_findings") == 1
    report = (tmp_path / "report.md").read_text(encoding="utf-8")
    assert "### F-high-1: the same defect, described" in report
    assert "F-low-1" in report  # absorbed, and still findable by name


# B-056 — the shape of the B-022 second review pass: five reviewers re-verified every first-pass
# finding BY MUTATION and marked the result. The consolidator returned NEEDS_FIXES with 5 HIGH, all
# five of them CLOSED in the same file it had just read. `grep -c status consolidate_findings.py`
# returned 0: the field was dropped in `_normalize_finding` and the verdict never saw it.
THREE_CLOSED_HIGHS = """agent_role: tests
findings:
  - id: F-tests-1
    status: CLOSED
    severity: HIGH
    file: src/a.ts
    line: 1
    summary: the upper half is now pinned; all four surviving mutants die
  - id: F-tests-2
    status: CLOSED
    severity: HIGH
    file: src/b.ts
    line: 2
    summary: the assertions now state the rule, not one example
  - id: F-arch-1
    status: CLOSED
    severity: HIGH
    file: src/c.ts
    line: 3
    summary: the edge is killed by four tests where it was killed by none
"""

THREE_OPEN_HIGHS = THREE_CLOSED_HIGHS.replace("    status: CLOSED\n", "")

ONE_CLOSED_ONE_OPEN = """agent_role: tests
findings:
  - id: F-closed
    status: CLOSED
    severity: HIGH
    file: src/a.ts
    line: 1
    summary: fixed and verified by mutation
  - id: F-open
    severity: BLOCKER
    file: src/b.ts
    line: 2
    summary: still broken
"""


def test_without_the_status_field_the_verdict_is_unchanged(tmp_path: Path) -> None:
    # THE CONTROL, and it is first on purpose: every findings file written before this change omits
    # `status`, and a fix that reinterpreted them would silently rescore every past review. It also
    # catches a "fix" that simply stopped counting HIGHs.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "tests.yml").write_text(THREE_OPEN_HIGHS, encoding="utf-8")

    code, payload = _run(findings, tmp_path / "report.md")

    # `NEEDS_FIXES` exits 1 by contract (`consolidate_findings.py:456`), so the exit code is part of
    # what "unchanged" means here.
    assert code == 1
    assert payload.get("verdict") == "NEEDS_FIXES"


def test_a_pass_that_closed_every_finding_does_not_score_as_a_failure(tmp_path: Path) -> None:
    # A second pass exists to answer one question — did the fixes work? — and the same three
    # findings, all closed, used to produce the identical verdict to the pass that found them.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "tests.yml").write_text(THREE_CLOSED_HIGHS, encoding="utf-8")

    code, payload = _run(findings, tmp_path / "report.md")

    assert code == 0
    assert payload.get("verdict") != "NEEDS_FIXES"
    assert payload.get("closed_count") == 3
    # Reported, never dropped: an empty report reads as "the reviewers found nothing", which is the
    # ambiguity B-019 was about.
    report = (tmp_path / "report.md").read_text(encoding="utf-8")
    assert "F-tests-1" in report


def test_an_open_finding_still_counts_beside_a_closed_one(tmp_path: Path) -> None:
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "tests.yml").write_text(ONE_CLOSED_ONE_OPEN, encoding="utf-8")

    code, payload = _run(findings, tmp_path / "report.md")

    assert code == 1  # the BLOCKER is open, and NEEDS_FIXES exits 1
    assert payload.get("verdict") == "NEEDS_FIXES"
    assert payload.get("closed_count") == 1


# B-042 — the ids `spawn_reviewers.py` mandates in every agent brief.
THREE_PREFIXED_HIGHS = """agent_role: mixed
findings:
  - id: F-arch-1
    severity: HIGH
    file: src/a.ts
    line: 1
    summary: an architectural debt worth carrying
  - id: F-tests-1
    severity: HIGH
    file: src/b.ts
    line: 2
    summary: a test gap worth carrying
  - id: F-dom-1
    severity: HIGH
    file: src/c.ts
    line: 3
    summary: a domain caveat worth carrying
"""

PLAN_REGISTERING_ALL = """# Plan: fixture

## Followups

- F-arch-1 — carried deliberately, owned.
- F-tests-1 — carried deliberately, owned.
- F-dom-1 — carried deliberately, owned.
"""

PLAN_REGISTERING_TWO = """# Plan: fixture

## Followups

- F-arch-1 — carried deliberately, owned.
- F-tests-1 — carried deliberately, owned.
"""


def _run_with_plan(findings_dir: Path, output: Path, plan: Path) -> tuple[int, dict[str, object]]:
    result = subprocess.run(
        [sys.executable, str(SCRIPT), "--findings-dir", str(findings_dir),
         "--output", str(output), "--slug", "fixture", "--plan", str(plan)],
        capture_output=True, text=True, check=False,
    )
    payload: dict[str, object] = {}
    if "{" in result.stdout:
        try:
            payload = json.loads(result.stdout[result.stdout.index("{"):])
        except json.JSONDecodeError:
            payload = {}
    return result.returncode, payload


def test_an_unregistered_high_still_blocks(tmp_path: Path) -> None:
    # THE CONTROL, and it passes before the fix: with one HIGH unregistered the verdict must stay
    # NEEDS_FIXES. A "fix" that registered everything would satisfy the test below and fail this.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "mixed.yml").write_text(THREE_PREFIXED_HIGHS, encoding="utf-8")
    plan = tmp_path / "plan.md"
    plan.write_text(PLAN_REGISTERING_TWO, encoding="utf-8")

    code, payload = _run_with_plan(findings, tmp_path / "report.md", plan)

    assert code == 1
    assert payload.get("verdict") == "NEEDS_FIXES"


def test_followups_registered_with_the_mandated_id_format_are_recognised(tmp_path: Path) -> None:
    # `\b[A-Za-z]+-\d+\b` applied to `F-arch-1` yields `arch-1`: the match cannot start at `F`,
    # because `F-` is not followed by digits. So no `F-`-prefixed id could EVER be registered, and
    # `READY_TO_MERGE_WITH_FOLLOWUPS` — a verdict `cycle-rule-schema.md` publishes and
    # `cycle-review.md § Verdicts` describes — was unreachable for every review this repo has run.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "mixed.yml").write_text(THREE_PREFIXED_HIGHS, encoding="utf-8")
    plan = tmp_path / "plan.md"
    plan.write_text(PLAN_REGISTERING_ALL, encoding="utf-8")

    code, payload = _run_with_plan(findings, tmp_path / "report.md", plan)

    assert payload.get("verdict") == "READY_TO_MERGE_WITH_FOLLOWUPS", payload
    assert not payload.get("unregistered_high")


def test_registration_is_case_insensitive(tmp_path: Path) -> None:
    # A plan is prose written by hand. `F-Arch-1` is a plausible typo, and a registration that fails
    # SILENTLY is the failure mode this item is about.
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "mixed.yml").write_text(THREE_PREFIXED_HIGHS, encoding="utf-8")
    plan = tmp_path / "plan.md"
    plan.write_text(
        PLAN_REGISTERING_ALL.replace("F-arch-1", "f-ARCH-1"), encoding="utf-8"
    )

    code, payload = _run_with_plan(findings, tmp_path / "report.md", plan)

    assert payload.get("verdict") == "READY_TO_MERGE_WITH_FOLLOWUPS", payload


# B-030 — six agents ran concurrently against ONE working tree during the B-025 review. At least two
# wrote to it: a mutant was found in `src/metrics/usage-panel.tsx` mid-run, and probe files appeared
# at the repo root. The architecture reviewer then read the mutated tree and filed a false BLOCKER
# claiming `reportGuardFailure` has no production call sites — it has two.
#
# Three of six independently noticed the tree was dirty and re-derived their citations. That they
# HAD to is the defect: correctness depended on each agent noticing.


def _repo_with_state(tmp_path: Path, dirty_after: bool) -> tuple[Path, Path]:
    """A findings dir carrying a recorded tree state, and the repo it describes."""
    repo = tmp_path / "repo"
    repo.mkdir()
    env = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t",
           "GIT_COMMITTER_EMAIL": "t@t", "PATH": "/usr/bin:/bin", "HOME": str(repo)}
    subprocess.run(["git", "init", "-q"], cwd=repo, check=True, capture_output=True, env=env)
    (repo / "a.txt").write_text("one\n", encoding="utf-8")
    subprocess.run(["git", "add", "-A"], cwd=repo, check=True, capture_output=True, env=env)
    subprocess.run(["git", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "seed"],
                   cwd=repo, check=True, capture_output=True, env=env)

    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "architecture.yml").write_text(VALID, encoding="utf-8")

    sys.path.insert(0, str(SCRIPT.parent))
    from consolidate_findings import record_tree_state  # noqa: PLC0415
    record_tree_state(repo, findings)

    if dirty_after:
        # Exactly what was observed: a file modified in place while other agents were reading.
        (repo / "a.txt").write_text("one\n// MUTANT: injected mid-review\n", encoding="utf-8")
    return repo, findings


def _run_in_repo(findings: Path, output: Path, repo: Path) -> tuple[int, dict[str, object]]:
    result = subprocess.run(
        [sys.executable, str(SCRIPT), "--findings-dir", str(findings),
         "--output", str(output), "--slug", "fixture", "--repo-root", str(repo)],
        capture_output=True, text=True, check=False,
    )
    payload: dict[str, object] = {}
    if "{" in result.stdout:
        try:
            payload = json.loads(result.stdout[result.stdout.index("{"):])
        except json.JSONDecodeError:
            payload = {}
    return result.returncode, payload


def test_an_unchanged_tree_reports_nothing(tmp_path: Path) -> None:
    # THE CONTROL, passing before the fix: a clean run must stay silent, or the signal is noise.
    repo, findings = _repo_with_state(tmp_path, dirty_after=False)

    code, payload = _run_in_repo(findings, tmp_path / "report.md", repo)

    assert code == 0
    assert not payload.get("tree_contaminated")


def test_a_tree_that_moved_during_the_run_is_reported(tmp_path: Path) -> None:
    # The B-025 shape: the tree the agents were reading changed while they read it.
    repo, findings = _repo_with_state(tmp_path, dirty_after=True)

    code, payload = _run_in_repo(findings, tmp_path / "report.md", repo)

    assert payload.get("tree_contaminated") is True
    report = (tmp_path / "report.md").read_text(encoding="utf-8")
    assert "contaminated" in report.lower()


def test_a_run_with_no_recorded_state_is_not_reported_as_contaminated(tmp_path: Path) -> None:
    # Every review directory written before this change has no state file. They must not become
    # retroactively contaminated — an absent record is not evidence of a change.
    repo = tmp_path / "repo"
    repo.mkdir()
    findings = tmp_path / "findings"
    findings.mkdir()
    (findings / "architecture.yml").write_text(VALID, encoding="utf-8")

    code, payload = _run_in_repo(findings, tmp_path / "report.md", repo)

    assert code == 0
    assert not payload.get("tree_contaminated")


def test_the_agents_own_findings_do_not_count_as_contamination(tmp_path: Path) -> None:
    # Measured while wiring B-030: `git status --porcelain` lists untracked files, and the findings
    # directory lives inside the repository. In this install `.claude/` is gitignored so it never
    # fired; in an install where the directory is tracked, EVERY review would report itself
    # contaminated by the findings the agents were spawned to write.
    #
    # A signal that always fires is noise — which is the whole reason the clean-tree control exists.
    repo = tmp_path / "repo"
    repo.mkdir()
    env = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t",
           "GIT_COMMITTER_EMAIL": "t@t", "PATH": "/usr/bin:/bin", "HOME": str(repo)}
    subprocess.run(["git", "init", "-q"], cwd=repo, check=True, capture_output=True, env=env)
    (repo / "a.txt").write_text("one\n", encoding="utf-8")
    subprocess.run(["git", "add", "-A"], cwd=repo, check=True, capture_output=True, env=env)
    subprocess.run(["git", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "seed"],
                   cwd=repo, check=True, capture_output=True, env=env)

    sys.path.insert(0, str(SCRIPT.parent))
    from consolidate_findings import check_tree_contamination, record_tree_state  # noqa: PLC0415

    findings = repo / "review" / "findings"   # INSIDE the repo, and not ignored
    record_tree_state(repo, findings)
    (findings / "architecture.yml").write_text(VALID, encoding="utf-8")

    assert check_tree_contamination(repo, findings) is None

    # …and it must still catch a change OUTSIDE the findings directory.
    (repo / "a.txt").write_text("one\n// MUTANT\n", encoding="utf-8")
    assert check_tree_contamination(repo, findings) is not None


def _seeded_repo(tmp_path: Path) -> tuple[Path, dict[str, str]]:
    repo = tmp_path / "repo"
    repo.mkdir()
    env = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t",
           "GIT_COMMITTER_EMAIL": "t@t", "PATH": "/usr/bin:/bin", "HOME": str(repo)}
    subprocess.run(["git", "init", "-q"], cwd=repo, check=True, capture_output=True, env=env)
    (repo / "a.txt").write_text("one\n", encoding="utf-8")
    subprocess.run(["git", "add", "-A"], cwd=repo, check=True, capture_output=True, env=env)
    subprocess.run(["git", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "seed"],
                   cwd=repo, check=True, capture_output=True, env=env)
    return repo, env


def test_a_sibling_sharing_the_findings_prefix_is_still_reported(tmp_path: Path) -> None:
    # The exclusion is a PATH exclusion, not a string prefix. `review/findings-probe.txt` is not in
    # `review/findings/` — it is a file an agent dropped next to it, which is the observed B-025
    # behaviour (probe files at the repo root).
    repo, _env = _seeded_repo(tmp_path)
    sys.path.insert(0, str(SCRIPT.parent))
    from consolidate_findings import check_tree_contamination, record_tree_state  # noqa: PLC0415

    findings = repo / "review" / "findings"
    record_tree_state(repo, findings)
    (findings / "architecture.yml").write_text(VALID, encoding="utf-8")

    (repo / "review" / "findings-probe.txt").write_text("probe\n", encoding="utf-8")

    assert check_tree_contamination(repo, findings) is not None


def test_a_rename_with_one_end_outside_the_findings_dir_is_reported(tmp_path: Path) -> None:
    # A tracked file MOVED into the findings directory: the reviewers' tree lost it. Only one end of
    # the rename sits under the exclusion, so the line must survive — excluding on either end would
    # let an agent hide a real mutation by moving the file it touched.
    repo, env = _seeded_repo(tmp_path)
    sys.path.insert(0, str(SCRIPT.parent))
    from consolidate_findings import check_tree_contamination, record_tree_state  # noqa: PLC0415

    findings = repo / "review" / "findings"
    record_tree_state(repo, findings)

    subprocess.run(["git", "mv", "a.txt", "review/findings/a.txt"],
                   cwd=repo, check=True, capture_output=True, env=env)

    assert check_tree_contamination(repo, findings) is not None
