# ADR 0011 — M31 re-scope: O(N)-gap closed now; latency-parity (SIMD) is M31b

**Status:** Accepted · **Data:** 2026-07-01 · **Owner:** CTO (paulohenriquevn) — re-scope decision
**Relacionado:** `.claude/knowledge-base/plans/m31-am-latency-plan.md`, `docs/benchmarks/m31-am-latency.{md,json}`,
`docs/adr/0010-m26-index-am-scope.md`, memória `goto-p0-vector-superiority`

## Contexto

M31 (track P0 — superioridade vetorial) reestruturou o `theodb_ivfflat` para **leitura parcial de páginas**
(meta + centroids + list pages; scan lê só as listas probed). Medição honesta (n=100k, dim=128, probes=10):

- M26 blob O(N)-por-scan: ~1700 ms · **M31 structured: ~38 ms** (≈ **45× mais rápido**) · pgvector: ~14 ms.
- Corretude 100% (recall preservado; manutenção INSERT/DELETE/VACUUM intacta; sem regressão — 49 testes verdes).

O gap **algorítmico** (O(N)→O(probes)) está **fechado** (theodb lê ~as mesmas páginas que o pgvector). O resíduo é
o **fator constante**: distância escalar/SSE2 (auto-vectorizada 4-wide) vs a **SIMD AVX 8-wide + dispatch de CPU em
runtime do pgvector (C, anos de tuning)**. O DoD original do plano ("p50 ≤ pgvector, band 1.5×") **NÃO bate por
evidência** (38 > 21).

## Decisão (CTO, 2026-07-01)

**Re-escopar M31 ao ganho MEDIDO e criar M31b para a paridade de latência via SIMD.**

- **M31 (agora):** o DoD passa a ser **"fechar o O(N)-por-scan (leitura parcial estruturada), com corretude +
  manutenção intactas e latência bem abaixo do regime O(N) e dentro de um band documentado do pgvector"** —
  provado por `benchmarks/tests/test_index_am_latency.py` (recall paridade · p50 << O(N) · p50 ≤ 4× pgvector). M31
  atinge READY_TO_MERGE nesse DoD. Isto entrega o valor real agora (45× vs M26; pré-requisito do M32 escala 1M+).
- **M31b (novo, no track P0, antes do M32):** **distância vetorial SIMD** (AVX2 + dispatch de CPU em runtime, ou
  crate portável tipo `wide`) para buscar **p50 ≤ pgvector** (paridade/superioridade de latência). É a fatia que
  fecha o resíduo do fator constante — com dep nova (deps-audit) e cuidado de portabilidade (dispatch).

## Alternativas rejeitadas

- **Grindar SIMD dentro do M31 até bater ≤ pgvector agora:** rejeitado pelo CTO — SIMD+dispatch é uma fatia própria
  (dep + portabilidade + incerteza com dados aleatórios); melhor entregar o ganho O(N)→parcial validado já e
  isolar o SIMD como M31b medível.
- **Falsear/afrouxar o benchmark p/ "passar" o DoD original:** proibido (Regra 3; `public-copy.md` — performance é
  claim só com evidência). O número honesto (2.7× atrás) fica registrado.
- **Descartar o structured-partial-read:** rejeitado — é correto, é 45× melhor que M26, e é a fundação para o SIMD
  (M31b otimiza a distância sobre as mesmas list pages).

## Consequências

- **Positivas:** o gap algorítmico fecha agora (medido); a base para 1M+ (M32) está pronta; honestidade preservada
  (paridade estrutural + algorítmica alcançada, latência-superior ainda meta — coerente com `goto-p0-vector-superiority`).
- **Negativas (aceitas):** theodb continua ~2.7× atrás do pgvector em latência até o M31b; documentado no benchmark.

## Atualiza

`ADR 0010 §D2/D5`: o O(N)-por-scan está **fechado para IVFFlat** (structured partial reads); a paridade de latência
(SIMD) migra de "follow-up genérico" para o milestone **M31b** rastreado no ROADMAP.
