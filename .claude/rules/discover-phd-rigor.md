# Discover PhD-Rigor Profile (TheoDB)

Per-project Source of Truth for the **rigor bar** the discovery cycle must clear on
TheoDB. TheoDB is a frontier project — an open-source, PostgreSQL-compatible database
that mirrors the SOTA of AlloyDB using only permissive OSS pieces. Discovery here is
not "read a couple of repos and summarize": it is **applied-PhD-grade investigation**
of the state of the art (algorithms, benchmarks, trade-offs) before any architectural
bet (e.g., the pgvector/pgvectorscale fork trigger of PRD D3).

This file raises the bar of `cycle-discover` **without** loosening any locked golden-rule
hard cap. It is cited by the discover skills and by the two discover golden rules
(`discover-plan-golden-rule.md`, `discover-blueprint-golden-rule.md`) under their
`§ Project rigor profile` sections.

## § 0 — Why this exists (the project mandate)

- **SOTA-anchored (CLAUDE.md, TheoDB rule 1).** Every technique investigated is positioned
  against the AlloyDB/ScaNN reference. A blueprint that does not say "AlloyDB does X; we
  match it with permissive piece Y; here is the gap" did not do frontier discovery.
- **Performance is a claim, not an opinion (CLAUDE.md, TheoDB rule 5).** Any latency/recall/
  throughput statement in a blueprint MUST carry the benchmark methodology + numbers from
  its primary source, or be explicitly flagged `UNBENCHMARKED`. No "X is faster" prose.
- **Fork only on reproducible evidence (PRD D3).** The fork of `pgvector`/`pgvectorscale`
  is authorized only when a reproducible benchmark justifies it. Discovery is where that
  evidence is gathered — so discovery MUST surface the benchmark, not assert the conclusion.
- **Esforço ≠ Complexidade (CLAUDE.md).** High effort here is welcome (read the paper, run
  the field comparison). The depth is *essential* complexity driven by the problem, never
  accidental ceremony. The bar deepens investigation — it never adds indirection.

## § 1 — The rigor bar (what "PhD applied" means here)

Applies to every discovery whose topic touches a performance-bearing or algorithm-bearing
pillar (P2 vector/AI, P3 columnar/HTAP, P4 HA/replication, P7 auto-tuning). For purely
operational/tooling topics (e.g., "what is the local-dev story"), the bar relaxes to the
baseline cycle contract.

| # | Requirement | Enforcement |
|---|---|---|
| **R0** | **REGRA MÁXIMA — busca web obrigatória (Paulo, 2026-07-08).** Todo deep research (fase `discover`, `/deep-research`, qualquer varredura SOTA) **DEVE** usar **WebSearch + WebFetch** para buscar evidência ativa em **papers (arXiv/venues), projetos open-source (repos/código real) e blogs técnicos especializados**, e citá-la. Conhecimento interno do modelo + leitura de código local **NÃO bastam** — um blueprint sem varredura web verificável é deep-research theatre e é rejeitado. Agentes despachados (council-*, general-purpose) DEVEM receber instrução explícita de usar WebSearch/WebFetch e citar. | Review-enforced (esta regra) + instrução obrigatória nos prompts de agentes de discover |
| R1 | **SOTA anchoring.** The `techniques` corner of the plan/blueprint names the AlloyDB (or field) SOTA approach for the same problem and states the gap TheoDB must close. | Review-enforced (this file) + `/discover-confidence` reads it |
| R2 | **>= 2 primary sources per technique.** Each technique claim cites >= 2 independent primary sources (a peer-reviewed paper, an official doc, or a maintained repo under `knowledge-base/references/`). A single blog/source is insufficient. | Review-enforced + partial: blueprint hard cap already requires >= 2 references overall (`discover-blueprint-golden-rule.md`) |
| R3 | **Benchmark evidence.** Every performance statement carries methodology + numbers + source, OR the literal marker `UNBENCHMARKED` (honest gap, becomes a next-discovery seed). Prose perf claims without either are rejected. | Review-enforced (this file); ties to `public-copy.md` + PRD D3 |
| R4 | **Technique depth.** The `techniques` corner carries **>= 2** research questions (frontier topics earn deeper interrogation of the algorithm than the 1-per-corner floor). | Authored target in `discover-plan/SKILL.md`; structural ceiling raised in `check_plan_completeness.py` (MAX_PER_CORNER=5) |
| R5 | **Authoritative sources only.** External WebFetch is restricted to `rules/discover-web-allowlist.txt`. A claim sourced outside the allowlist is not citable. | Script-adjacent: the allowlist gates `/discover-execute` WebFetch |
| R6 | **Honest BLOCKED over false COMPLETE.** When the field evidence is not reachable (paywalled paper, no permissive equivalent of a SOTA piece), the question is marked `BLOCKED` with reason — never padded with a fabricated answer (Unbreakable Rule 3). | Existing halt-loop contract (`cycle-discover.md`) |

## § 2 — Question budget (frontier profile)

Frontier topics need more interrogation room than the generic 5–10 default. The structural
window is widened (but still bounded — investigation, not a research project):

- **Total: 6–14 questions** (was 5–10). Sweet spot 8–10 for a pillar-sized topic.
- **Max 5 per corner** (was 3) — to allow the `techniques` corner to go deep.
- **Techniques corner: >= 2 questions** (R4) — the SOTA axis is where TheoDB's bets live.
- The locked hard cap "question count <= 15" (`discover-plan-golden-rule.md`) is unchanged
  and still bounds the window; 14 stays inside it.

## § 3 — What this profile does NOT change (honesty)

- It does **not** add a 5th coverage corner. SOTA-anchoring lives inside the existing
  `techniques` corner (KISS — no ripple into the hardcoded corner list of the checker scripts).
- It does **not** weaken any locked hard cap (fabricated citation, empty corner, etc.).
- It does **not** auto-enforce R1/R2/R3 with new Python today — those are **review-enforced**
  and read by `/discover-confidence`. Promoting any of them to a deterministic checker is a
  future slice (would require a fixture-backed script + an ADR), tracked as honest debt here
  rather than faked with a phantom script reference.

## § 4 — When this profile may change

This is a per-project rules file (not a locked golden rule), so it may be tuned freely with a
CHANGELOG entry. Promoting any review-enforced requirement (R1–R3) to a hard/soft cap inside a
locked golden rule additionally requires an ADR (per each golden rule's `§ When this rule may
change`). The current promotion ADR is `knowledge-base/adrs/0001-discover-phd-rigor.md`.

## Cross-references

- Cycle: `cycle-discover.md` (the cycle this profile sharpens)
- Locked golden rules it feeds: `discover-plan-golden-rule.md`, `discover-blueprint-golden-rule.md`
- Allowlist (R5): `discover-web-allowlist.txt`
- Skills that read it: `skills/discover-plan/SKILL.md`, `skills/discover-execute/SKILL.md`
- Project mandate: `../../CLAUDE.md` (TheoDB rules 1, 5; "Esforço ≠ Complexidade"), `PRD.md` D3
- Honesty + copy: `public-copy.md`, Unbreakable Rule 3
- ADR: `knowledge-base/adrs/0001-discover-phd-rigor.md`
