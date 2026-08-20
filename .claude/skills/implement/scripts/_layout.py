"""Where this install keeps its knowledge-base.

B-032 — `mini_review.py` and `check_phase_review.py` both defaulted to `knowledge-base/…`, the
STANDALONE layout, in an ecosystem where every consumer is a plugin install. Running the mini review
with defaults therefore created a second knowledge-base at the project root, beside the one every
other cycle writes to.

`rules/knowledge-base-location.md` states the rule and its measured cost: three consumers in 2026-08
where an audit read `.claude/` and reported "0 implementations, 0 reviews, 0 releases" for a
repository that had 6, 12 and 8. Its own words: *"an audit trail split across two directories is
worse than none: a reader who checks the wrong one reports absence where evidence exists."*

ONE resolver for both scripts, deliberately. The split was quiet because the WRITER and the READER
agreed with each other while both disagreed with the ecosystem; two copies of this branch could
drift into disagreeing, which is louder but no better. One fact, one implementation.

KNOWN DUPLICATION, recorded rather than hidden: `skills/release/scripts/flip_milestone_checkbox.py`
(`_default_runs_dir`) already carries the same four-line branch, for the same reason, with a
docstring that already named this failure. That makes this the third occurrence and the rule of
three says extract — but the third lives in another SKILL, and a module shared across skill
boundaries is a structural decision this bug fix did not measure. Left as two copies on purpose;
the merge is registered as a followup.
"""
from __future__ import annotations

from pathlib import Path


def knowledge_base_root(project_root: Path) -> Path:
    """`.claude/knowledge-base` in a plugin install, `knowledge-base` in the standalone kit.

    Detected from the tree, never taken as a flag. The item's decisive evidence is a recurrence:
    the defect was known for eight mini-review runs because the operator passed the flag every
    time, and the ninth time they did not, the split came back. A default that depends on
    remembering IS the defect.
    """
    if (project_root / ".claude").exists():
        return project_root / ".claude" / "knowledge-base"
    return project_root / "knowledge-base"


def default_mini_reviews_dir(project_root: Path) -> Path:
    """The canonical mini-review directory for this install's layout."""
    return knowledge_base_root(project_root) / "mini-reviews"
