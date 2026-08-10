---
type: Decision
title: ADR 0011 — M31 re-escopado: o gap O(N) fecha agora; paridade de latência (SIMD) vira M31b
description: A leitura parcial de páginas fecha o gap algorítmico (45× vs M26), mas fica 2,7× atrás do pgvector por fator constante SIMD; o CTO re-escopou o milestone em vez de afrouxar o critério.
resource: git:f7c7b93:docs/adr/0011-m31-rescope-simd-followup.md
tags: [adr, index-am, ivfflat, simd, latencia, m31, honestidade]
adr_id: "0011"
adr_status: Accepted
decision_date: 2026-07-01
owner: human:paulohenriquevn
milestone: M31
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0011
    resource: git:f7c7b93:docs/adr/0011-m31-rescope-simd-followup.md
    title: ADR 0011 — M31 re-scope
    last_modified: 2026-07-01
---

O caso didático da regra "performance é claim, não opinião": o critério de pronto original **não
bateu por evidência**, e a resposta foi re-escopar o milestone ao ganho medido — não afrouxar o
benchmark.

# Contexto

O M31 reestruturou o `theodb_ivfflat` para **leitura parcial de páginas**: meta, centroides e
list pages, com o scan lendo apenas as listas sondadas. Medição honesta (n=100k, dim=128,
`probes=10`):

| Configuração | p50 |
|---|---|
| M26, blob O(N) por scan | ~1700 ms |
| **M31, structured partial read** | **~38 ms** (≈ **45×** mais rápido) |
| pgvector | ~14 ms |

Correção 100%: recall preservado, manutenção INSERT/DELETE/VACUUM intacta, 49 testes verdes.

O gap **algorítmico** — O(N) para O(probes) — está **fechado**: o TheoDB lê aproximadamente as
mesmas páginas que o pgvector. O resíduo é **fator constante**: distância escalar/SSE2
auto-vetorizada 4-wide, contra a SIMD AVX 8-wide com dispatch de CPU em runtime do pgvector, em C
e com anos de tuning. O critério original do plano — "p50 ≤ pgvector, banda de 1,5×" — **não bate**
(38 > 21).

# Decisão do CTO

Re-escopar o M31 ao ganho **medido** e criar o M31b para a paridade de latência via SIMD.

- **M31 (agora):** o critério passa a ser fechar o O(N) por scan com correção e manutenção
  intactas e latência bem abaixo do regime O(N), dentro de uma banda documentada do pgvector
  (recall em paridade, p50 muito abaixo do O(N), p50 ≤ 4× pgvector). Entrega o valor real agora
  e é pré-requisito da escala 1M+ do M32.
- **M31b (novo):** distância vetorial SIMD — AVX2 com dispatch de CPU em runtime, ou crate
  portável — buscando `p50 ≤ pgvector`. Fecha o resíduo de fator constante, com dependência nova
  e cuidado de portabilidade.

# Alternativas rejeitadas

**Grindar SIMD dentro do M31 até bater ≤ pgvector.** Rejeitada pelo CTO: SIMD com dispatch é uma
fatia própria — dependência, portabilidade, incerteza com dados aleatórios. Melhor entregar o
ganho O(N)→parcial validado e isolar o SIMD como algo medível.

**Falsear ou afrouxar o benchmark para "passar" o critério original.** Proibido. O número honesto
— 2,7× atrás — fica registrado.

**Descartar o structured partial read.** Rejeitada: é correto, é 45× melhor que o M26, e é a
fundação sobre a qual o SIMD otimiza, sobre as mesmas list pages.

# Consequências

O gap algorítmico fecha agora, medido; a base para 1M+ está pronta; e a honestidade é preservada
— paridade estrutural e algorítmica alcançadas, latência superior ainda meta. Aceita-se
permanecer ~2,7× atrás do pgvector em latência até o M31b, documentado no benchmark
([m31](/benchmarks/m31-am-latency.md), [m31b](/benchmarks/m31b-simd-distance.md)).

Atualiza o [ADR 0010](/decisions/0010-m26-index-am-scope.md) §D2/D5: o O(N) por scan está fechado
para o IVFFlat, e a paridade de latência migra de "follow-up genérico" para milestone
rastreado.[^adr0011]

[^adr0011]: ADR 0011 — M31 re-scope: O(N)-gap closed now; latency-parity is M31b
