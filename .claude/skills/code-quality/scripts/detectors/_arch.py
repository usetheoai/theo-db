"""D5 — the vocabulary shared by every language's architecture detector.

## Why Squad ships no layer model

An architecture rule is knowledge of the project, not of the tool that checks it. A canned
"domain must not import infrastructure" would fail repos for an architecture nobody declared,
and the resulting findings would be evidence of nothing — the same defect gate G5 rejects when
prior art is offered as justification.

So D5 runs the config the REPO declares, in the linter's own native format
(`.dependency-cruiser.cjs`, `.go-arch-lint.yml`, `Layerfile.toml`). Squad invents no fourth
format: the tools already have one, and a translation layer would be a dialect to maintain for
no gain. A repo with no config is SKIPPED with an INFO finding — not failed.

## What D5 adds on top of the linters

The meta-gate: **a rule that cannot fail is worse than no rule.** It transfers confidence to a
mechanism that stopped existing, and the reader has no way to tell the difference.

Two measurements, both from this ecosystem:

1. `theo-contracts`, 2026-08-06. A `.go-arch-lint.yml` whose component named a directory that
   does not exist reports `ArchHasWarnings: false` — green — with the diagnosis demoted to
   `ExecutionWarnings`, a field nothing was reading. The rule could no longer fire and the
   pipeline said it passed.

2. `usetheo-labs/agent-builder`, 2026-08-06. Its `.dependency-cruiser.cjs` records the same class
   in prose: dissolving `tui/lib` into seven capability folders would leave a `forbidden` rule
   written against the old literal path matching nothing — *"uma regra `forbidden` que nao casa
   nada produz ZERO violacoes, ou seja, VERDE. O invariante evaporaria em silencio."*

The shape is identical in both: the rule names a directory, the directory moves, the gate goes
quiet instead of red. D5 asserts the directories a rule names still exist.

## What D5 deliberately does NOT assert

That the rule's TARGET matches something. A `forbidden` rule whose `to` matches nothing is
usually the rule working: `no-sdk-direto` forbids importing a package precisely so that nobody
imports it. Demanding a match there would force every preventive rule to be violated once to be
believed.
"""
from __future__ import annotations

from scripts._shared import Finding

D5 = "d5_architecture"

#: How many characters a rule's reason needs before it counts as a reason.
#:
#: Twenty, adopted from the journeys registry of usetheo-labs/agent-builder, which states the
#: calibration: short enough not to demand prose, long enough to exclude `n/a`, `-` and `interno`
#: — the three ways of not answering.
RAZAO_MINIMA = 20


def reason_has_substance(reason: str | None) -> bool:
    """A reason is present and says something. `None`, blank and `n/a` are all "not answered"."""
    return reason is not None and len(reason.strip()) >= RAZAO_MINIMA


def violation(
    language: str, *, tool: str, rule: str, file_path: str, symbol_or_line: str, message: str
) -> Finding:
    """The repo declared this rule and the code breaks it. HARD — that is what declaring means."""
    return Finding(
        detector=D5,
        language=language,
        severity="HARD",
        file_path=file_path,
        symbol_or_line=symbol_or_line,
        message=f"[{tool}] {rule}: {message}",
        allowlist_key=f"{language}|{file_path}|architecture|{rule}",
    )


def vacuous_rule(language: str, *, tool: str, rule: str, config_path: str, detail: str) -> Finding:
    """A rule that names something no longer in the tree.

    HARD, and deliberately the same severity as a real violation. The invariant is gone either
    way; the difference is that this failure mode reports success on the way out.
    """
    return Finding(
        detector=D5,
        language=language,
        severity="HARD",
        file_path=config_path,
        symbol_or_line=rule,
        message=(
            f"[{tool}] rule '{rule}' names something that is not in the tree: {detail}. "
            "The rule can no longer fire, and it fails GREEN — nothing else would report this."
        ),
        allowlist_key=f"{language}|{config_path}|architecture|vacuous_{rule}",
    )


def rule_without_reason(language: str, *, tool: str, rule: str, config_path: str) -> Finding:
    """A rule nobody explained.

    SOFT_FLOOR, not HARD: the invariant still holds, so the code is fine. What is missing is the
    ability of the next reader to tell a deliberate boundary from a leftover — which is the
    difference between a rule that survives a refactor and one that gets deleted as noise.
    """
    return Finding(
        detector=D5,
        language=language,
        severity="SOFT_FLOOR",
        file_path=config_path,
        symbol_or_line=rule,
        message=(
            f"[{tool}] rule '{rule}' carries no reason of at least {RAZAO_MINIMA} characters. "
            "Say what it protects and what measured it — a boundary without a why is deleted by "
            "the next person who finds it inconvenient."
        ),
        allowlist_key=f"{language}|{config_path}|architecture|unexplained_{rule}",
    )


def no_config(language: str, *, tool: str, looked_for: list[str]) -> Finding:
    """No architecture declared here.

    INFO, and this is the point: Squad has no opinion about this repo's layers until someone
    writes one down. Failing here would be Squad asserting an architecture nobody agreed to.
    """
    return Finding(
        detector=D5,
        language=language,
        severity="INFO",
        file_path=".",
        symbol_or_line=tool,
        message=(
            f"no architecture rules declared ({tool} config not found: {', '.join(looked_for)}). "
            "D5 skipped — a rule the repo did not declare is not a rule Squad may enforce."
        ),
        allowlist_key=f"{language}|.|architecture|no_config_{tool}",
    )


def auditor_unavailable(language: str, *, tool: str, reason: str) -> Finding:
    """The rules exist and could not be checked. SOFT_CAP, matching D1's contract."""
    return Finding(
        detector=D5,
        language=language,
        severity="SOFT_CAP",
        file_path=".",
        symbol_or_line=tool,
        message=f"{tool} auditor unavailable: {reason}",
        allowlist_key=f"{language}|.|architecture|auditor_unavailable_{tool}",
    )
