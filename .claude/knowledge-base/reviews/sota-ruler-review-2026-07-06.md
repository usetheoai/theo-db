# /review — M50 régua SOTA vetorial

Date: 2026-07-06 · Slug: `sota-ruler` · milestone_id: M50 · Commits: `c53f9d4..HEAD` (após v0.40.0)

## Verdict: READY_TO_MERGE (após fixes de review)

Dois council specialists; ambos READY_TO_MERGE após correções.

## Reviewers + findings

**council-benchmark: NEEDS_FIXES → READY_TO_MERGE** — lente "você mediu ou está supondo?". Harness metodologicamente sólido (GT exato seqscan, dados distintos não-ADR-0012, apples-to-apples theodb↔pgvector, índices isolados, sem cherry-pick). 6 findings, resolvidos:
- **[HIGH F1]** tabela §1 do `.md` tinha p50/qps de um run diferente do JSON committado (edição à mão) → **FIXED**: tabela regenerada do JSON + teste `test_md_table_numbers_match_json_no_handedit` que cross-checa as 12 linhas (recall/p50/qps) contra `per_spec`. Re-auditor verificou célula-a-célula: batem.
- **[MED F2]** caveat de recall_std subestimado (~0.006-0.011 vs real 0.024) → **FIXED**: "≤ 0.024 (≤ 0.007 no ef=400)".
- **[MED F3]** "~40% atrás" multi-cliente vs real 29% @8c → **FIXED**: "29% a 8c, ~14% a 16c".
- **[LOW F4]** "1.7×" apresentado como estável vs 1.26-2.10× entre runs → **FIXED**: "1.64× ± 0.35 same-run".
- **[LOW F5]** teste não pegava divergência md↔json → **FIXED**: o novo teste é exatamente essa guarda.

**council-vector-ann: NEEDS_FIXES → READY_TO_MERGE** — correção técnica do veredito-gate M51. 4 findings, resolvidos:
- **[HIGH Q3]** dependência do recall-gate justificada pelo teto M39 (0.77-0.95) que o próprio `m40-ceiling-probe.md` **falsificou** (recall é carrier-limited, não quantizer-limited) e que o objetivo do M51 rejeita → **FIXED**: re-baseado em M40 + `sbq.rs:6-7`; citações verificadas fiéis pelo re-auditor.
- **[MED Q1]** eixo QPS (pressão de memória) e eixo recall (carrier) fundidos na explicação do diskann → **FIXED** em §4 e §1 (linha 43, segundo passe).
- **[LOW Q2]** analogia "M51 parecerá regressão como diskann" não-escopada → **FIXED**: escopada só ao throughput; recall <0.99 a 25k = falha REAL.
- **[LOW Q4]** faltava condição de box-quieta no gate de QPS → **FIXED**: 3ª condição de medição.

## Evidence (image theodb:m49-p3, n=25000 dim=128 cosine 3 runs)
- Régua 3-way: theodb_hnsw recall-parity com pgvector (0.941 vs 0.935), ~1.6-1.7× atrás em latência (fator-constante), 29%/14% menos QPS a 8c/16c; diskann dominado a 25k (0.877, 43ms, build 69s).
- Primeiro QPS multi-cliente de banco (8/16 conns) medido.
- Veredito-gate M51: AUTORIZADO + re-escopado (3 condições de medição herdadas).
- Higiene G8: JSON m41/m43, banner ADR-0012 em m32, superseded M31, reconcile M30.
- Testes de contrato: 4 passed.

## Hard gates
Failing tests: NENHUM (contrato 4/4; os errors de integração de outros milestones precisam de containers próprios, não-relacionados). Sem secrets; sem commit em main; sem Co-Authored-By; CHANGELOG atualizado.

## Caveats honestos (não bloqueantes — decisão explícita do usuário)
Escala reduzida (25k×128 gaussiano vs cohere 1M do DoD) + box contendida (load 7.9→12.6) — o veredito RELATIVO é robusto ao ruído (consistente em 1c+multi+3runs); latência ABSOLUTA carrega ruído (documentado). Full-scale gated no streaming build (M55+)/box dedicada.

**Verdict:** READY_TO_MERGE
