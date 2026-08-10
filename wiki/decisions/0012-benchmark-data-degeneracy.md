---
type: Decision
title: ADR 0012 — Dados de benchmark DEVEM ser distintos (a degenerescência do InitPlan-hoist)
description: Um idiom SQL fez 100.000 linhas receberem o vetor idêntico, invalidando retroativamente todos os números de latência anteriores ao M31b.
resource: git:f7c7b93:docs/adr/0012-benchmark-data-degeneracy.md
tags: [adr, benchmark, metodologia, rigor, honestidade, m31b]
adr_id: "0012"
adr_status: Accepted
decision_date: 2026-07-01
milestone: M31b
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0012
    resource: git:f7c7b93:docs/adr/0012-benchmark-data-degeneracy.md
    title: ADR 0012 — Benchmark data must be DISTINCT
    last_modified: 2026-07-01
---

O ADR mais importante de metodologia do repositório: um defeito no *harness* — não no engine —
que fez todo número de latência publicado antes do M31b medir a coisa errada. É citado como
precedente por praticamente toda decisão de medição posterior.

# O sintoma

Perfilando o scan do `theodb_ivfflat`, o profiler opt-in `THEODB_SCAN_PROFILE=1` reportou, numa
tabela de 100.000 linhas em dim=128:

```
theodb scan profile: cand=100000 nonempty_lists=1/100 probes=10 reads=13764us score=3191us sort=91us
```

O scan pontuava **todas as 100.000 linhas** — não as ~10.000 esperadas de sondar 10 de 100
listas — porque o k-means colocara **todos os vetores numa única lista**.

# A causa-raiz

A investigação **inocentou o TheoDB**: uma reprodução standalone do mesmo k-means (k-means++,
10 iterações de Lloyd, SplitMix64 com seed 42, distâncias f32) sobre vetores genuinamente
distintos produz listas **balanceadas** (máximo ≈ 1069; 10 probes → ≈ 10.035 candidatos).

O culpado era o SQL de geração de dados do benchmark:

```sql
INSERT INTO lat SELECT g, ('['||(SELECT string_agg((random())::text, ',')
                                 FROM generate_series(1,128))||']')::vector
FROM generate_series(1,100000) g;
```

O sub-select interno **não referencia a linha externa `g`**, então o PostgreSQL o trata como
**InitPlan não-correlacionado e o avalia exatamente uma vez** — a volatilidade de `random()` não
força reavaliação por linha de um sub-select não-correlacionado. Resultado: as 100.000 linhas
receberam o **vetor IDÊNTICO** (`COUNT(DISTINCT embedding::text) = 1`, verificado). `LATERAL` não
resolveu, por continuar sem referenciar `g`.

Pontos idênticos colapsam *qualquer* k-means correto numa única lista não-vazia, e o recall é
trivialmente 10/10 — cada linha é um empate exato. **A degenerescência era portanto invisível ao
gate de recall** e produziu um workload de força-bruta-sobre-empates fantasiado de ANN.[^adr0012]

# Decisão

1. **Dados de benchmark DEVEM ser distintos.** Vetores são semeados em Python
   (`random.Random(seed)`) e carregados via `COPY`; nunca pelo idiom `string_agg((random())…)`.
   A geração **assere `COUNT(DISTINCT) == N` antes do uso**.
2. **Dois regimes são medidos:** uniforme-aleatório (pior caso do IVFFlat) e gaussiano-clusterizado
   (realista, tipo-embedding).
3. **As figuras de latência do M31 são retro-invalidadas** como medições ANN — elas mediram
   empates de vetores idênticos. Os números corrigidos do
   [M31b](/benchmarks/m31b-simd-distance.md) as supersedem. A conquista **estrutural** do
   [M31](/decisions/0011-m31-rescope-simd-followup.md) — leitura parcial O(N)→O(probes) — segue
   válida; sua comparação de fator constante é que estava sobre dados ruins.
4. **O profiler permanece**, desligado por padrão, como observabilidade permanente de balanço de
   listas: `nonempty_lists` próximo de `1/100` sobre dados distintos é o tripwire desta classe de
   bug.

# Consequências

**Positiva:** o resultado verdadeiro do M31b é bem mais forte que o quadro anterior — sobre dados
distintos, o TheoDB é **2,6× mais rápido que o pgvector** (uniforme, recall em paridade) e ≤
pgvector em recall pleno (clusterizado). A narrativa dos "2,7× atrás" era artefato de dados ruins.

**Negativa e honesta:** um benchmark publicado carregou números inválidos. Este ADR registra a
correção em vez de sobrescrever a história silenciosamente. **Não havia bug de código no
engine** — o defeito estava no harness de teste.

# Alternativas rejeitadas

**SQL correlacionado** (`WHERE g = g`, `+ 0*g`) para derrotar o hoist — frágil, porque o planner
pode dobrar a correlação como constante, e opaco a um revisor. O `COPY` de dados explicitamente
distintos é inequívoco e reproduzível. **Manter só dados uniformes** — uniforme é o pior caso do
IVFFlat e subestima o recall real; o regime clusterizado é necessário para medir o ponto de
operação realista.

[^adr0012]: ADR 0012 — Benchmark data must be DISTINCT (the InitPlan-hoist degeneracy)
