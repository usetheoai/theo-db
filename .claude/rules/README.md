# Rules

Source of truth for cycle contracts, golden rules, thresholds, and allowlists.
Every cycle reads its contract from here; every quality gate references a golden
rule file.

## Cycle Contracts

Each `cycle-{name}.md` defines:
- Entry conditions and prerequisites
- Phase sequence with advance criteria
- Hard gates (BLOCKER-level) and soft gates (advisory)
- Cross-references to skills, hooks, and scripts
- Verdicts vocabulary (e.g., SHIPPABLE_WITH_CAVEATS, READY_TO_MERGE)

| Contract | Cycle | Key Verdicts |
|---|---|---|
| `cycle-backlog.md` | Intake (phase 0) | ITEM_REGISTERED / ITEM_REJECTED |
| `cycle-maintenance.md` | Macro super-loop | ITEM_SHIPPED / ITEM_KILLED / BACKLOG_EMPTY |
| `cycle-discover.md` | Measurement of our own system | SHIPPABLE_WITH_CAVEATS / ITEM_KILLED |
| `cycle-plan.md` | Planning | SHIPPABLE_WITH_CAVEATS |
| `cycle-implement.md` | Implementation | IMPLEMENTATION_COMPLETE |
| `cycle-code-quality.md` | Code quality audit | PASS, PASS_WITH_CAVEATS, FAIL_SOFT, FAIL_HARD, INVALID |
| `cycle-review.md` | Multi-agent review | READY_TO_MERGE, NEEDS_FIXES, NEEDS_DEEPER |
| `cycle-release.md` | Release cut | RELEASED, PR_OPEN_AWAITING_APPROVAL |
| `cycle-analysis.md` | Trajectory analysis (opt-in, post-release) | ON_TRACK, COURSE_CORRECTION_NEEDED |
| `cycle-judge-codex.md` | External Codex jury (optional plugin) | SHIPPABLE, READY_TO_MERGE |
| `cycle-auto-plan.md` | Auto-orchestrator | Delegates to sub-cycles |

## Golden Rules (locked severity rubrics)

| File | Purpose |
|---|---|
| `code-quality-golden-rule.md` | Code quality severity levels |
| `discover-opportunity-golden-rule.md` | Opportunity confidence hard caps |
| `plan-confidence-golden-rule.md` | Plan confidence scoring rubric |
| `discover-plan-golden-rule.md` | Discovery plan scoring rubric |
| `deps-audit-golden-rule.md` | Dependency audit severity |
| `dogfood-golden-rule.md` | Anchor scenario + status vocab |
| `analysis-golden-rule.md` | Trajectory analysis modules + verdict caps |

## Thresholds and Allowlists

| File | Purpose |
|---|---|
| `code-quality-thresholds.txt` | Per-project threshold overrides |
| `code-quality-allowlist.txt` | Findings exemptions (mandatory sunset) |
| `code-quality-languages.txt` | Enabled languages per project |
| `plan-confidence-thresholds.txt` | Plan scoring thresholds |
| `plan-confidence-allowlist.txt` | Plan findings exemptions |
| `discover-web-allowlist.txt` | Authoritative domains for WebFetch |
| `deps-audit-allowlist.txt` | Dependency audit exemptions |
| `discover-opportunity-thresholds.txt` | Opportunity confidence thresholds |
| `live-target.txt` | Declared live environments per domain (live-test refuses without one) |
| `current-constraint.md` | The constraint lens — advisory, never a gate |
| `discover-plan-thresholds.txt` | Discovery plan scoring thresholds |
| `analysis-config.txt` | Trajectory analysis profile + enablement |
| `review-model-routing.txt` | Agent model routing for review |

## Other Rules

| File | Purpose |
|---|---|
| `cycle-rule-schema.md` | Canonical schema + verdict matrix for all `cycle-*.md` |
| `architecture.md` | Layering and DIP boundaries |
| `testing.md` | TDD discipline and pyramid |
| `error-handling.md` | Fail-fast discipline, typed errors (Unbreakable Rule 8) |
| `git-safety.md` | Forbidden git commands + safe substitutes (Unbreakable Rule 4) |
| `reference-provenance.md` | Keeping third-party study material out of the project (4 layers) |
| `parsimony-ladder.md` | Pre-write minimalism ladder (YAGNI/KISS/Don't-Reinvent) enforced in GREEN phase |
| `public-copy.md` | Banned framings in README/marketing |
| `audit-trail-rotation.md` | When to archive/delete artifacts |
| `loop-engine-convention.md` | Skill vs Agent vs ralph-loop |

## Consertos que vivem só aqui, e quem fica exposto

Este `.claude/` é um **checkout do kit `squad`**, versionado dentro deste repositório. Um conserto
escrito aqui protege exatamente este projeto. O repositório do kit
(`git@github.com:paulohenriquevn/squad.git`) **não** é atualizado a partir daqui — decisão do owner
em 2026-08-20, com o custo declarado em vez de implícito.

O registro abaixo existe porque a alternativa é pior: um conserto não portado e não anotado vira um
defeito que alguém reencontra do zero, sem saber que já foi diagnosticado.

| Conserto | Item | Quem fica exposto |
|---|---|---|
| Resolução de caminho ciente de layout nos três checadores de evidência (`check_evidence_pointers.py`, `check_evidence_citations.py`, `check_measurement_targets.py`), delegando ao `scripts/ecosystem_utils.py` | [[B-081]] | **Todo consumidor do kit em layout de plugin.** Sem isto, uma oportunidade ou plano que cite um arquivo do ecossistema (`rules/…`) é acusado de **evidência fabricada** — o hard cap mais severo da skill, disparado sobre trabalho legítimo. Invisível no kit, porque lá `rules/` está na raiz. |
| `check_coverage_matrix` reporta `coverage_lt_100` em vez de levantar `ValueError` | [[B-081]] | Todo consumidor: um plano sem `## Coverage Matrix` **derruba** o `run_structural` em vez de reprovar, e quem chama não distingue "plano inválido" de "ferramenta quebrada". |
| `check_intake_gates.py` repassa `--rule` ao `route_domain.py` | [[B-077]] | Consumidor cuja tabela de roteamento não está no caminho default: o gate G1 recusa todo item filado. |

**Como isto sai daqui, se um dia sair:** os diffs estão nos commits que citam o item entre parênteses
no `CHANGELOG.md`. Nenhum deles depende de nada específico deste projeto.

## Modifying Rules

- Cycle contracts and golden rules are **locked** — changes require team discussion
- Thresholds and allowlists are per-project and can be adjusted freely
- Run `python3 scripts/check_xrefs.py` after any change to validate references
