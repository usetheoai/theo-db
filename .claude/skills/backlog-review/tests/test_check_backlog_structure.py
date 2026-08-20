"""Tests for check_backlog_structure.py — the ways a maintenance registry rots."""
from __future__ import annotations

from datetime import date
from pathlib import Path

import pytest

from check_backlog_structure import check_backlog
from helpers import item_block, write_backlog


def _checks(report: dict) -> set[str]:
    return {f["check"] for f in report["findings"]}


def _find(report: dict, check: str) -> dict:
    return next(f for f in report["findings"] if f["check"] == check)


def test_clean_backlog_is_shippable(clean_backlog: Path) -> None:
    report = check_backlog(clean_backlog)
    assert report["verdict"] == "SHIPPABLE", report["findings"]
    assert report["items_total"] == 2


def test_status_counts(clean_backlog: Path) -> None:
    report = check_backlog(clean_backlog)
    assert report["items_by_status"]["raw"] == 1
    assert report["items_by_status"]["triaged"] == 1


def test_duplicate_id_is_a_blocker(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block("B-001"), item_block("B-001", "Outro")))
    assert "duplicate_id" in _checks(report)
    assert report["verdict"] == "INVALID"


def test_ids_out_of_file_order_are_not_a_blocker(tmp_path: Path) -> None:
    """Position in the file is not reuse, and only reuse breaks traceability.

    The check used to be `renumbered`, a deterministic BLOCKER whose message said a
    reused id "makes every earlier reference ambiguous". Measured on this project's own
    registry on 2026-08-20: no id was reused (`uniq -d` empty) and three pairs merely sat
    out of ascending order, because blocks had been inserted next to related items. Every
    `[[B-NNN]]` reference still resolved to exactly one block, so the harm the message
    asserted had not occurred. Reuse is already caught deterministically by
    `duplicate_id`, which leaves position with nothing of its own to prove.
    """
    report = check_backlog(write_backlog(tmp_path, item_block("B-005"), item_block("B-002", "Outro")))
    assert "renumbered" not in _checks(report)
    assert report["verdict"] != "INVALID", report["findings"]


def test_ids_out_of_file_order_are_reported_as_a_heuristic_minor(tmp_path: Path) -> None:
    """Still worth saying — a registry read top to bottom is easier when ids ascend."""
    report = check_backlog(write_backlog(tmp_path, item_block("B-005"), item_block("B-002", "Outro")))
    finding = _find(report, "ids_out_of_order")
    assert finding["severity"] == "minor"
    assert finding["kind"] == "heuristic"
    assert "reused" not in finding["message"], "the message must not assert a harm that did not occur"


def test_ascending_ids_produce_no_order_finding(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block("B-002"), item_block("B-005", "Outro")))
    assert "ids_out_of_order" not in _checks(report)


def test_a_killed_item_needs_no_dod(tmp_path: Path) -> None:
    """It closed by `kill_reason`; a closing criterion is moot after the close.

    `thin_dod` says "nothing states when this item is done, so it never closes". For an
    item that already closed, the consequence is contradicted by the block itself — and
    the only way to satisfy the check would be to invent a closing criterion after the
    fact, which is grading a moved target.
    """
    killed = item_block("B-002", "Morto no mesmo ciclo", status="killed", dod=[],
                        extra="kill_reason: consertado no ciclo em que foi descoberto\n")
    report = check_backlog(write_backlog(tmp_path, item_block("B-001"), killed))
    assert "thin_dod" not in _checks(report)


def test_an_open_item_still_needs_a_dod(tmp_path: Path) -> None:
    """The exemption is for closed items only — an open one without a criterion never closes."""
    report = check_backlog(write_backlog(tmp_path, item_block("B-001", dod=[])))
    assert "thin_dod" in _checks(report)


def test_triaged_without_evidence_is_a_blocker(tmp_path: Path) -> None:
    """Triaged means measured. Without evidence the status is a claim nobody made."""
    report = check_backlog(write_backlog(tmp_path, item_block(status="triaged", evidence="none-yet")))
    assert "triaged_without_evidence" in _checks(report)
    assert report["verdict"] == "INVALID"


def test_raw_carrying_evidence_is_flagged(tmp_path: Path) -> None:
    """Measurement happened and nobody advanced the status — the rot this loop prevents."""
    report = check_backlog(write_backlog(tmp_path, item_block(status="raw", evidence="src/x.ts:12")))
    assert "raw_with_evidence" in _checks(report)


def test_killed_without_reason_is_flagged(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block(status="killed")))
    assert "killed_without_reason" in _checks(report)


def test_killed_with_reason_is_clean(tmp_path: Path) -> None:
    report = check_backlog(
        write_backlog(tmp_path, item_block(status="killed", extra="kill_reason: medido, 1 query por request\n"))
    )
    assert "killed_without_reason" not in _checks(report)


def test_illegal_status_is_a_blocker(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block(status="in-progress")))
    assert "illegal_status" in _checks(report)
    assert report["verdict"] == "INVALID"


def test_invalid_mode_is_flagged(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block(suggested_mode="vibes")))
    assert "invalid_mode" in _checks(report)


def test_missing_field_is_flagged(tmp_path: Path) -> None:
    block = item_block().replace("why_now: o dashboard passou a carregar 30d por padrão\n", "")
    report = check_backlog(write_backlog(tmp_path, block))
    assert "missing_field" in _checks(report)


ROUTING_TABLE = """# Cycle: BACKLOG

## Domain routing

| Domain | Repos | Specialist |
|---|---|---|
| `data-plane-ts` | `theo-lens`, `theo-memory` | `agents/data-plane-ts.md` |
| `platform-cli` | `theo-cli` | `agents/platform-cli.md` |

## Next
"""


def _with_routing_table(tmp_path: Path) -> Path:
    """Plant a real routing table next to the backlog so the G1 check actually runs."""
    rules = tmp_path / "rules"
    rules.mkdir(exist_ok=True)
    (rules / "cycle-backlog.md").write_text(ROUTING_TABLE, encoding="utf-8")
    return tmp_path


def test_unroutable_repo_is_a_blocker(tmp_path: Path) -> None:
    """A repo in no domain routes to nobody — gate G1.

    Plants a routing table so the check is genuinely exercised. Without one the checker
    correctly declines to judge, and the assertion would pass for the wrong reason: the
    check never ran.
    """
    _with_routing_table(tmp_path)
    report = check_backlog(write_backlog(tmp_path, item_block(repo="theo-gateway")))
    assert report["routing_table_read"] is True
    assert "unroutable_repo" in _checks(report)
    assert report["verdict"] == "INVALID"


def test_routable_repo_produces_no_finding(tmp_path: Path) -> None:
    _with_routing_table(tmp_path)
    report = check_backlog(write_backlog(tmp_path, item_block(repo="theo-lens")))
    assert report["routing_table_read"] is True
    assert "unroutable_repo" not in _checks(report)


def test_unreadable_routing_table_does_not_assert_violations(tmp_path: Path) -> None:
    """Missing data must not become a reported violation.

    With no routing table, every repo would look unroutable. Reporting that would assert
    a violation the evidence does not support — the same defect the thresholds resolver
    had when it silently used the wrong bands.
    """
    report = check_backlog(write_backlog(tmp_path, item_block(repo="anything-at-all")))
    assert report["routing_table_read"] is False
    assert "unroutable_repo" not in _checks(report)


def test_thin_dod_is_flagged(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block(dod=[])))
    assert "thin_dod" in _checks(report)


def test_vague_dod_is_heuristic_not_deterministic(tmp_path: Path) -> None:
    report = check_backlog(write_backlog(tmp_path, item_block(dod=["melhorar a performance"])))
    assert "vague_dod" in _checks(report)
    assert _find(report, "vague_dod")["kind"] == "heuristic"


def test_dod_with_a_number_is_not_vague(tmp_path: Path) -> None:
    """A criterion that happens to mention speed is still falsifiable.

    Flagging "p95 below 800ms" because it contains "fast"-adjacent wording would train
    people to ignore the check, which costs more than the false negatives it prevents.
    """
    report = check_backlog(write_backlog(tmp_path, item_block(dod=["p95 abaixo de 800ms, mais rápido que hoje"])))
    assert "vague_dod" not in _checks(report)


def test_stale_raw_is_flagged(tmp_path: Path) -> None:
    report = check_backlog(
        write_backlog(tmp_path, item_block(status="raw", registered="2026-01-01")),
        today=date(2026, 8, 5),
    )
    assert "stale_raw" in _checks(report)
    assert _find(report, "stale_raw")["kind"] == "heuristic"


def test_recent_raw_is_not_stale(tmp_path: Path) -> None:
    report = check_backlog(
        write_backlog(tmp_path, item_block(status="raw", registered="2026-08-01")),
        today=date(2026, 8, 5),
    )
    assert "stale_raw" not in _checks(report)


def test_possible_duplicate_between_open_items(tmp_path: Path) -> None:
    report = check_backlog(
        write_backlog(
            tmp_path,
            item_block("B-001", "Reduzir round-trips do listing de traces"),
            item_block("B-002", "Reduzir round-trips no listing de traces do explorer"),
        )
    )
    assert "possible_duplicate" in _checks(report)


def test_closed_items_are_not_duplicate_candidates(tmp_path: Path) -> None:
    """A shipped item and a new one about the same area is normal — that is a follow-up.

    Only OPEN items compete for the same work; flagging closed ones would make every
    recurring area look duplicated forever.
    """
    report = check_backlog(
        write_backlog(
            tmp_path,
            item_block("B-001", "Reduzir round-trips do listing de traces", status="shipped"),
            item_block("B-002", "Reduzir round-trips no listing de traces do explorer"),
        )
    )
    assert "possible_duplicate" not in _checks(report)


def test_verdict_is_derived_from_findings(tmp_path: Path) -> None:
    """The verdict is computed, never asserted — same discipline as the scorers."""
    blocker = check_backlog(write_backlog(tmp_path, item_block(status="bogus")))
    assert blocker["severity_counts"]["blocker"] >= 1 and blocker["verdict"] == "INVALID"

    major = check_backlog(write_backlog(tmp_path, item_block(dod=[])))
    assert major["severity_counts"]["blocker"] == 0 and major["verdict"] == "NEEDS_REVISION"

    minor = check_backlog(write_backlog(tmp_path, item_block(dod=["melhorar a performance"])))
    assert minor["severity_counts"]["major"] == 0 and minor["verdict"] == "SHIPPABLE_WITH_CAVEATS"


def test_every_finding_declares_its_kind(tmp_path: Path) -> None:
    """A reader must be able to tell "the machine is sure" from "a human should look"."""
    report = check_backlog(
        write_backlog(tmp_path, item_block(status="bogus", dod=["melhorar tudo"], suggested_mode="vibes"))
    )
    assert report["findings"]
    for f in report["findings"]:
        assert f["kind"] in ("deterministic", "heuristic"), f


def test_a_duplicated_status_is_a_blocker(tmp_path: Path) -> None:
    """Two `status:` lines leave the block with two answers, and every reader takes the last.

    Measured on theo-db: B-021 carries `raw` then `triaged`, B-022 `planned` then `raw`. The
    index buckets on status, so an ambiguous one makes the summary arbitrary rather than
    wrong-in-a-way-you-can-see.
    """
    backlog = write_backlog(tmp_path, item_block("B-001", status="raw", extra="status: triaged\n"))
    report = check_backlog(backlog)
    dup = [f for f in report["findings"] if f["check"] == "duplicate_field"]
    assert len(dup) == 1
    assert dup[0]["severity"] == "blocker"
    assert "raw then triaged" in dup[0]["message"]


def test_other_repeated_fields_are_not_reported(tmp_path: Path) -> None:
    """`partial_progress` four times on theo-cloud's B-031 is an append-one-line-per-increment
    log the team keeps on purpose, and `evidence: none-yet` followed by a pointer is an item that
    advanced. A gate that reports those is one people learn to override."""
    backlog = write_backlog(
        tmp_path,
        item_block("B-001", extra="partial_progress: primeira metade\npartial_progress: segunda\n"),
    )
    report = check_backlog(backlog)
    assert [f for f in report["findings"] if f["check"] == "duplicate_field"] == []


# ---------------------------------------------------------------------------
# B-051 — o checkbox e o `status` são dois campos do mesmo bloco que podem
# discordar em silêncio. Nada os comparava, e a divergência sobreviveu meses.
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("status", ["raw", "triaged", "planned"])
def test_checked_box_over_an_open_status_is_a_blocker(tmp_path: Path, status: str) -> None:
    """`[x]` diz "fechado" e `raw`/`triaged`/`planned` dizem "aberto". Nenhum dos dois está errado
    isolado, e é por isso que a divergência sobrevive: só a COMPARAÇÃO a revela.

    Medido no `theo-db` de `842347c`: o B-001 carregava `[x]` com `status: raw` desde 2026-08-10 e
    atravessou três leituras do backlog. Em 2026-08-20 a mesma classe reapareceu duas vezes no mesmo
    dia (B-012 e B-015), e o custo foi concreto — eu ia reimplementar trabalho já lançado.
    """
    backlog = write_backlog(tmp_path, item_block("B-001", status=status, checkbox="x"))
    report = check_backlog(backlog)
    div = [f for f in report["findings"] if f["check"] == "checkbox_status_divergent"]
    assert len(div) == 1, f"[x] com status {status} deve ser reportado"
    assert div[0]["severity"] == "blocker"
    assert div[0]["kind"] == "deterministic"
    assert status in div[0]["message"]


def test_unchecked_box_over_shipped_is_a_blocker(tmp_path: Path) -> None:
    """A direção oposta e igualmente silenciosa: o trabalho foi lançado e o checkbox ficou para trás."""
    backlog = write_backlog(tmp_path, item_block("B-001", status="shipped", checkbox=" "))
    report = check_backlog(backlog)
    div = [f for f in report["findings"] if f["check"] == "checkbox_status_divergent"]
    assert len(div) == 1
    assert div[0]["severity"] == "blocker"


@pytest.mark.parametrize(
    ("status", "checkbox"),
    [("shipped", "x"), ("raw", " "), ("triaged", " "), ("planned", " "), ("killed", "x"), ("killed", " ")],
)
def test_aligned_checkbox_and_status_produce_no_finding(
    tmp_path: Path, status: str, checkbox: str
) -> None:
    """O portão só afirma as duas direções que o DoD do B-051 nomeia.

    `killed` fica de fora DELIBERADAMENTE, nos dois checkboxes: o contrato em `cycle-backlog.md`
    não diz qual marca um item morto carrega, e inventar a regra aqui produziria um veredito sobre
    convenção que nenhuma decisão sustenta — que é o mesmo defeito que o gate existe para impedir.
    """
    extra = "kill_reason: a medição não sustentou a hipótese\n" if status == "killed" else ""
    backlog = write_backlog(
        tmp_path, item_block("B-001", status=status, checkbox=checkbox, extra=extra)
    )
    report = check_backlog(backlog)
    assert [f for f in report["findings"] if f["check"] == "checkbox_status_divergent"] == []


def test_planned_without_evidence_is_a_major(tmp_path: Path) -> None:
    """`raw → planned` sem passar por `triaged` é proibido em texto e ninguém verificava.

    O histórico não está no arquivo, então a transição em si não é observável. O que É observável
    é o rastro que ela deixa: `triaged` exige evidência (gate já existente), logo um `planned` com
    `evidence: none-yet` prova que a medição foi pulada. É o proxy honesto — reporta o rastro, não
    a transição que não pode ver.
    """
    backlog = write_backlog(
        tmp_path, item_block("B-001", status="planned", checkbox=" ", evidence="none-yet")
    )
    report = check_backlog(backlog)
    skipped = [f for f in report["findings"] if f["check"] == "planned_without_evidence"]
    assert len(skipped) == 1
    assert skipped[0]["severity"] == "major"
    assert skipped[0]["kind"] == "deterministic"


def test_planned_with_evidence_is_clean(tmp_path: Path) -> None:
    backlog = write_backlog(
        tmp_path,
        item_block("B-001", status="planned", checkbox=" ", evidence="wiki/benchmarks/x.md"),
    )
    report = check_backlog(backlog)
    assert [f for f in report["findings"] if f["check"] == "planned_without_evidence"] == []


# ---------------------------------------------------------------------------
# B-051 bullet 2 — `shipped` exige evidência de release, não a palavra `shipped`.
# ---------------------------------------------------------------------------


def test_shipped_citing_a_commit_outside_any_semver_tag_is_a_major(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """O erro que o próprio B-051 registra: marcar `shipped` no dia da implementação, com o
    release ainda em `PR_OPEN_AWAITING_APPROVAL`. O commit existe; a release não.
    """
    import check_backlog_structure as mod

    monkeypatch.setattr(mod, "_semver_tags_containing", lambda repo, sha: [])
    backlog = write_backlog(
        tmp_path,
        item_block("B-001", status="shipped", checkbox="x", extra="entregue em `deadbeef`\n"),
    )
    report = check_backlog(backlog)
    found = [f for f in report["findings"] if f["check"] == "shipped_without_release_evidence"]
    assert len(found) == 1
    assert found[0]["severity"] == "major"
    assert "deadbeef" in found[0]["message"]


def test_shipped_citing_a_commit_inside_a_semver_tag_is_clean(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import check_backlog_structure as mod

    monkeypatch.setattr(mod, "_semver_tags_containing", lambda repo, sha: ["v0.1.0"])
    backlog = write_backlog(
        tmp_path,
        item_block("B-001", status="shipped", checkbox="x", extra="entregue em `deadbeef`\n"),
    )
    report = check_backlog(backlog)
    assert [f for f in report["findings"] if f["check"] == "shipped_without_release_evidence"] == []


def test_a_shipped_block_citing_no_commit_is_counted_as_unverified_not_clean(
    tmp_path: Path,
) -> None:
    """Sem ponteiro no bloco não há o que verificar, e o honesto é DIZER isso.

    Reprovar seria afirmar um defeito que a evidência não sustenta; passar em silêncio seria
    alegar cobertura que não houve — o mesmo `cobertura-alegada-sem-execucao` que o acervo já
    registra. O relatório carrega a contagem, e quem lê decide.
    """
    backlog = write_backlog(tmp_path, item_block("B-001", status="shipped", checkbox="x"))
    report = check_backlog(backlog)
    assert [f for f in report["findings"] if f["check"] == "shipped_without_release_evidence"] == []
    assert report["shipped_without_verifiable_pointer"] == 1


def test_open_items_are_never_asked_for_release_evidence(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import check_backlog_structure as mod

    monkeypatch.setattr(mod, "_semver_tags_containing", lambda repo, sha: [])
    backlog = write_backlog(
        tmp_path, item_block("B-001", status="raw", extra="visto em `deadbeef`\n")
    )
    report = check_backlog(backlog)
    assert [f for f in report["findings"] if f["check"] == "shipped_without_release_evidence"] == []
    assert report["shipped_without_verifiable_pointer"] == 0
