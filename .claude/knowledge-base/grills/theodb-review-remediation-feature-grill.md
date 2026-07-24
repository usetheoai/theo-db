---
slug: theodb-review-remediation
generated_by: roadmap-feature
date: 2026-07-23
status: completed
milestones_added: [M146, M147]
source: review-cycle full-tree theodb_rs (knowledge-base/review-archive/theodb-full-core-complete-2026-07-23/)
issues: ["#168", "#169", "#170"]
---

# Grill — theodb-review-remediation (M146 + M147)

Milestones criados a partir dos findings do `/review-cycle:loop` full-tree do `theodb_rs`
(núcleo: 12 arquivos mais críticos × 10/10 pilares, 32 findings, precision 1.00, 0 blockers).
O owner escolheu granularidade **B (2 milestones)** — espelha o split M144 (remediação) / M145
(refactor). Respostas derivadas do review com alta confiança (evidência + issues), não por
interrogatório, pois o owner delegou explicitamente ("Crie milestones para corrigir TODOS os
pontos encontrados") e o conteúdo está ancorado nos findings medidos.

## Q1 — O que é e por que agora?

Remediação dos pontos acionáveis do review full-tree do theodb_rs (2026-07-23). **Por que agora:**
o review acabou de surfar os findings; corrigi-los antes de mais trabalho de feature mantém o crate
limpo (o núcleo já é maduro — a maioria é INFO/applied_correctly; estes são os poucos acionáveis).
Split em 2 por tipo de risco: **M146 = fixes/hardening/tests/cleanup** (baixo risco, TDD direto);
**M147 = refactor** (comportamento-preservado, exige A/B byte-idêntico — risco distinto).

## Q2 — Dependências

- **M146** gated por **M145** (o refactor de CC já estabilizou vectorizer.rs/parquet.rs/scan.rs).
- **M147** gated por **M146** — o #169 remove `scan_hnsw_structured` de `scan.rs` antes do refactor
  de dispatch tocar o mesmo arquivo (evita conflito de escopo/merge).

## Q3 — Definition of Done

Ver blocos M146/M147 no ROADMAP.md. M146: #168 regclass + from_bytes validation + #169 dead-code +
test-gaps (ivf mod tests, soar, doc-drift) + parquet fsync. M147: dispatch-table + Vec→Result helpers
+ Stage-1 in-memory compartilhado, A/B byte-idêntico por-versão, ADR-2 preservada.

## Q4 — Top-2 riscos NOVOS

- **M146:** (a) teste de injection #168 exige droplet (`cargo pgrx test` inexecutável local); (b)
  validação de from_bytes pode rejeitar blobs M26 deprecados legítimos.
- **M147:** (a) regressão recall/QPS ao unificar Stage-1; (b) tentação de unificar corpos por-versão
  (viola ADR-2, risco data-loss).

## Out-of-scope cross-check

Sem overlap — remediação de findings de review é trabalho de qualidade interna, não uma feature
nova contra itens declarados out-of-scope (HA/replicação/sharding/columnar-paridade permanecem fora).

## SOTA delta

Não — os findings são internos ao crate; nenhum peer de referência novo é necessário.
