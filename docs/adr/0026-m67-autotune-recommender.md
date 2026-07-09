# ADR 0026 — M67 auto-tune de índices: recomendador determinístico + coletor de stats; auto-tune online deferido

**Status:** Accepted · **Data:** 2026-07-09 · **Milestone:** M67 · **Owner:** Eng
**Relacionado:** blueprint `.claude/knowledge-base/discoveries/blueprints/m67-autotune-blueprint.md`,
plan `.claude/knowledge-base/plans/m67-autotune-plan.md`, ADRs do M48 (amcostestimate honesto), M35 (imutabilidade
do grafo), `.claude/rules/parsimony-ladder.md`, `.claude/rules/error-handling.md`, Unbreakable Rule 9.

## Contexto

`ef_search`/`probes` são knobs manuais; um banco maduro auto-ajusta pela workload (P7). A discovery (blueprint,
R0 web-citado, ≥2 fontes por claim) concluiu: **quase nenhum sistema de produção auto-tuna ef ONLINE** — o SOTA
é early-termination query-adaptativo acadêmico (DARTH [arXiv:2505.19001], Ada-ef [arXiv:2512.06636]); o único
auto-tuner shipado (VDTuner [arXiv:2404.10413]) é Bayesian offline recomendador. E **grande parte da
instrumentação já existe** (`reads` counter `hnsw_page.rs:1515`, cost.rs honesto do M48, sinal de convergência M52).

## Decisão D1 — Recomendador determinístico (bisecção monotônica); NÃO auto-tune online

`theodb.recommend_ef(index_table regclass, vector_col text, sample_queries text[], recall_target float DEFAULT 0.95, k int DEFAULT 10) RETURNS int`:
para cada query da amostra computa o GT exato (seqscan brute force); depois faz **doubling** `[k,2k,4k,…]` até
recall(ef) ≥ alvo, e **bisecta** o bracket para o **menor ef** que ainda atinge o alvo. Read-only. O operador
aplica com `SET theodb_hnsw.ef_search`.

**Rationale:** recall(ef) é **monotônico não-decrescente** (a lista de candidatos de ef+1 é superset da de ef —
Malkov & Yashunin [arXiv:1603.09320]) → a bisecção é sã (sem máximos locais). Auto-tune que muta o ef vivo
oscila, colide com o `SET` do usuário, e é difícil de tornar crash-safe/observável (nenhum vector-DB de produção
faz). O DoD permite "auto-tune **ou** recomendação". `ctid` é o identificador de linha estável (sem precisar do PK).

**Alternativas rejeitadas:**
- **(A) Auto-tune online (mutar ef_search por feedback)** — oscilação, afeta queries em voo. Rejeitada.
- **(B) Early-termination query-adaptativo (Ada-ef/DARTH)** — SOTA (6.8-13.6× DARTH) mas probabilístico +
  (DARTH) modelo GBDT + pipeline de treino offline. Deferido para v2 (Ada-ef rule-based é a entrada de menor risco).

## Decisão D2 — Coletor de stats num catálogo heap (fora das páginas do índice); crash-safety

Catálogo `theodb._index_scan_stats (relid oid PK, n_scans, sum_pages_read, sum_latency_us, last_ef, last_updated)`
(molde `vectorizer_worker_stats`). `theodb.scan_stats(tbl, col, query, ef, k)` mede 1 scan e retorna o
**pages_read REAL** (de um thread_local backend-local que o `traverse` do HNSW bumpa — 1 add em memória, sem page
write) + latência, persistindo a observação. `theodb.index_scan_stats(rel)` lê os agregados.

**Rationale:** escrever stat nas páginas do índice via GenericXLog a cada scan violaria partial-read + a
imutabilidade do grafo (M35) — write-amp no read path. O catálogo heap é crash-safe e mantém o scan das páginas
do índice **read-only** (contrato IndexAmRoutine intacto). Amostragem: o coletor grava quando chamado, não a cada
hot-path scan (custo).

**Alternativas rejeitadas:**
- **(A) Stats nas páginas do índice** — viola crash-safety/partial-read (M35). Rejeitada.
- **(B) Write SPI por-scan no hot path** — caro/transaccionalmente delicado. Rejeitada (amostragem é KISS).

## Decisão D3 — amcostestimate: fórmula M48 honesta retida + auditabilidade; calibração-in-planning DEFERIDA

A fórmula do `amcostestimate` (M48, `cost.rs` — visit-ratio f(ef) porta do pgvector hnsw.c) é **retida** (já é
honesta, f(ef)). O coletor (`theodb.scan_stats`) dá **auditabilidade real**: o operador compara o custo estimado
(f(ef)) contra o pages_read REAL medido. A **calibração automática do amcostestimate com a stat empírica é
DEFERIDA** por risco.

**Rationale (honestidade — não é workaround):** ler o catálogo de stats via SPI DENTRO do `amcostestimate` (que
roda no planning) violaria o contrato EC-3 do M48 ("amcostestimate NUNCA pode dar error, senão aborta TODO o
planejamento de queries"). Um SPI no planning enquanto o VACUUM torna o catálogo/meta momentaneamente ilegível
abortaria o planejamento de TODAS as queries — uma regressão inaceitável. O valor honesto entregue é a
**auditabilidade** (medir o real vs o estimado), não uma auto-calibração arriscada. A calibração segura (ex.:
ler um agregado de shared-memory sem SPI) é um bet v2 medido.

## Edge/negative

- `recall_target ∉ (0,1]` / `k ≤ 0` / sample vazio → typed error 22023 (fail-fast).
- Alvo inatingível dentro do ef_max (ex. 0.999) → retorna MAX_EF (o operador vê o teto — honesto, não crash).
- `scan_stats` com ef/k ≤ 0 → 22023.

## Evidência (medida)

- **5 pg_test GREEN (stack real):** recommend_ef (monotonicidade + bounded + validação de target/sample) +
  scan_stats (pages_read real > 0 + persistência no catálogo + validação). 12 pytest da aritmética de convergência.
- **Benchmark de convergência (10k sintético) — CONVERGED com nuance:** o recomendador converge (retorna o menor
  ef; recall médio 0.986 ≥ alvos). **Ressalvas honestas:** (1) corpus fácil (baseline ef=64 dá 1.0; todos os alvos
  → ef=10 — não estressa a curva ef; SIFT1M mostraria o scaling); (2) RQUT 12% (cauda) — o recomendador é
  mean-optimal, não tail-safe (v2). `docs/benchmarks/m67-autotune.{md,json}`.

## Consequências

- **Fim do knob manual** — o operador chama `theodb.recommend_ef` e aplica o ef sugerido (estável, auditável).
- **Custo auditável** — `theodb.scan_stats` mede o pages_read real (fecha a auditabilidade do gap M48/cost).
- **Auto-tune online DEFERIDO** por evidência (oscilação); early-termination adaptativo é v2 medido.

## Caveats honestos

Recall-est do recomendador usa GT exato amostrado (a base honesta) — não um estimador GT-free (não-confiável). A
convergência é dependente do corpus (diminishing returns: 0.999 exige ef super-linear — honest-negative onde
inatingível). A calibração automática do cost é deferida por risco (D3), não entregue.
