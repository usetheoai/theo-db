---
type: Decision
title: ADR 0027 — Observabilidade do query vetorial por função diagnóstica, não por hook de EXPLAIN
description: O PostgreSQL não tem hook amexplain, então o diagnóstico por query vira theodb.explain_scan, expondo candidates_seen — o sinal que separa grafo caro de I/O pesado.
resource: git:f7c7b93:docs/adr/0027-m68-vector-observability.md
tags: [adr, observabilidade, explain, diagnostico, operabilidade, m68]
adr_id: "0027"
adr_status: Accepted
decision_date: 2026-07-09
milestone: M68
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0027
    resource: git:f7c7b93:docs/adr/0027-m68-vector-observability.md
    title: ADR 0027 — M68 observabilidade do query vetorial
    last_modified: 2026-07-09
---

O scan ANN é **opaco por natureza**: nem pgvector nem pgvectorscale expõem, por query, quantos nós o
beam navegou ou quantas páginas leu. Um operador com "recall ruim em produção" adivinha o `ef`,
tenta valores e reza. Este ADR entrega o instrumento.

# D1 — é uma função diagnóstica, não um hook de `EXPLAIN`

O PostgreSQL **não tem** hook para um access method injetar linhas no `EXPLAIN` do plano —
`amexplain` não existe no PG17 nem no PG18. A única forma seria um hook C de planner/executor:
indireção pesada, frágil a cada major, e fora do contrato do `IndexAmRoutine`.

Adota-se o padrão da indústria de vector DBs — Qdrant expõe `/telemetry`, Milvus expõe métricas de
segmento. Aqui:

```sql
theodb.explain_scan(index_table, vector_col, query, ef, k)
```

que retorna, de **um scan real**: `index_name`, `ef_effective`, `pages_read`, `candidates_seen`,
`latency_us` e `results`. É portável, honesto (não finge ser o `EXPLAIN` do plano) e suficiente
para o diagnóstico.

**Rejeitado:** hook C de executor injetando no `EXPLAIN` — manutenção alta, privado do core, muda
entre majors, e o ganho sobre uma função diagnóstica é cosmético.

# D2 — `candidates_seen` é capturado, não estimado

O motor de busca próprio já mantém o conjunto `visited` do beam. Captura-se `visited.len()` **antes
do drop** e propaga-se para um contador backend-local, irmão do de páginas lidas. É a **verdade do
que o scan navegou**, não um proxy.

**Ressalva honesta:** no caminho aproximado (SBQ/AQ), `candidates_seen` reflete o pool alargado do
walk (`ef · over_fetch`), não o `ef` do resultado. É o número certo — o que o beam de fato tocou —
mas o operador precisa saber lê-lo.

# D3 — a métrica runtime é catálogo consultável, não histograma Prometheus

Em vez de introduzir dependência de exporter — que seria escopo de plataforma, fora deste
repositório, que é o banco —, a métrica é a coluna `sum_candidates` no catálogo heap
`theodb._index_scan_stats`, agregada por `theodb.index_scan_stats(rel)`. O catálogo vive em heap,
fora das páginas de índice, logo é crash-safe. Um exporter por cima é passo trivial de plataforma,
adiado por não haver consumidor.

# Consequências

**Positivas:** o operador ganha três instrumentos que pgvector e pgvectorscale não têm —
`explain_scan` por query, `scan_stats` por query com persistência, e `index_scan_stats` agregado. E
`candidates_seen` **distingue as duas causas de latência alta** que `pages_read` sozinho não separa:
grafo caro de navegar (candidates alto) contra I/O pesado e spill para disco (pages alto). Zero
dependência nova, zero fork.

**Ressalvas honestas:** não é o `EXPLAIN` do plano, então quem espera ver as linhas dentro de
`EXPLAIN ANALYZE` não as verá. O `ef_effective` é o `ef` **passado**, não o crescido pelo iterative
scan, que vive no executor real; para esse, o operador olha `last_ef` nos agregados. E a métrica é
catálogo, não série temporal.

**Validação:** por teste funcional determinístico, **sem benchmark de performance** — nenhuma
afirmação de "Nx mais rápido" é feita, então a regra de claim não se aplica.[^adr0027]

# Relação com o north star

Operabilidade é pilar de produto, não o pilar de superioridade vetorial. Este milestone não avança o
claim de performance; entrega o instrumento que um operador de produção precisa e que os
concorrentes OSS diretos não expõem por query. O runbook correspondente é
[diagnóstico de scan vetorial](/runbooks/vector-scan-diagnostics.md).

[^adr0027]: ADR 0027 — M68: observabilidade do query vetorial via função diagnóstica
