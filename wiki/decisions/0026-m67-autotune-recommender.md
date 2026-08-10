---
type: Decision
title: ADR 0026 — Auto-tune de índices: recomendador determinístico + coletor de stats; auto-tune online adiado
description: theodb.recommend_ef acha o menor ef que atinge o recall-alvo por bisecção monotônica; mutar o ef vivo foi rejeitado, e a calibração automática do cost model foi adiada por risco de abortar o planejamento.
resource: git:f7c7b93:docs/adr/0026-m67-autotune-recommender.md
tags: [adr, autotune, ef-search, cost-model, observabilidade, m67]
adr_id: "0026"
adr_status: Accepted
decision_date: 2026-07-09
milestone: M67
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0026
    resource: git:f7c7b93:docs/adr/0026-m67-autotune-recommender.md
    title: ADR 0026 — M67 auto-tune de índices
    last_modified: 2026-07-09
---

# Contexto

`ef_search` e `probes` eram knobs manuais, e um banco maduro se auto-ajusta pela workload. A
investigação encontrou um fato que redirecionou o desenho: **quase nenhum sistema de produção
auto-ajusta o `ef` online**. O estado da arte publicado é early-termination adaptativo por query, e
o único auto-tuner efetivamente embarcado no mercado é um recomendador Bayesiano **offline**.

# Decisão D1 — recomendador determinístico por bisecção; não auto-tune online

```sql
theodb.recommend_ef(index_table regclass, vector_col text, sample_queries text[],
                    recall_target float DEFAULT 0.95, k int DEFAULT 10) RETURNS int
```

Para cada query da amostra computa o ground truth exato por força bruta; depois faz **doubling**
(`k, 2k, 4k…`) até `recall(ef) ≥ alvo` e **bisecta** o bracket para achar o **menor ef** que ainda
atinge o alvo. É read-only; o operador aplica o resultado com `SET`.

O que torna a bisecção sã: `recall(ef)` é **monotônico não-decrescente**, porque a lista de
candidatos de `ef+1` é superconjunto da de `ef` — propriedade do [HNSW](/technologies/hnsw.md). Sem
máximos locais, a busca é correta.

**Auto-tune online foi rejeitado:** mutar o `ef` vivo oscila, colide com o `SET` do usuário, afeta
queries em voo e é difícil de tornar crash-safe e observável — nenhum vector DB de produção faz. O
early-termination adaptativo é SOTA (ganhos publicados de 6,8 a 13,6×) mas é probabilístico e, na
variante mais forte, exige modelo GBDT com pipeline de treino offline; ficou como aposta futura.

# Decisão D2 — stats num catálogo heap, fora das páginas do índice

O catálogo `theodb._index_scan_stats` guarda contagem de scans, soma de páginas lidas, soma de
latência, último `ef` e timestamp. A função `theodb.scan_stats` mede um scan e retorna o
**pages_read REAL** — vindo de um contador backend-local que o traverse do HNSW incrementa, um
simples add em memória, sem escrita de página — mais a latência, persistindo a observação.

Escrever estatística **nas páginas do índice** violaria a leitura parcial e a imutabilidade do
grafo, criando amplificação de escrita no caminho de leitura. O catálogo heap é crash-safe e mantém
o scan das páginas do índice **read-only**, deixando o contrato do `IndexAmRoutine` intacto. A
amostragem grava quando chamada, não a cada scan de hot-path.

# Decisão D3 — cost model honesto retido; calibração automática adiada por risco

A fórmula do `amcostestimate` é **retida** por já ser honesta. O coletor dá **auditabilidade real**:
o operador compara o custo estimado contra o `pages_read` medido.

A **calibração automática está adiada, e não é workaround**. Ler o catálogo de stats via SPI
*dentro* do `amcostestimate` — que roda no planejamento — violaria o contrato de que
`amcostestimate` **nunca pode dar erro, senão aborta TODO o planejamento de queries**. Um SPI no
planejamento enquanto o VACUUM torna o catálogo momentaneamente ilegível abortaria o planejamento
de **todas** as queries: regressão inaceitável. O valor honesto entregue é a auditabilidade, não uma
auto-calibração arriscada.[^adr0026]

# Casos negativos

`recall_target` fora de (0,1], `k ≤ 0` ou amostra vazia viram erro tipado. Alvo inatingível dentro
do teto de `ef` retorna o máximo — o operador vê o teto, o que é honesto e não é crash.

# Evidência

Testes verdes na stack real cobrindo monotonicidade, limites e validação, mais `pages_read` real
maior que zero e persistência no catálogo.

**Benchmark de convergência a 10k** ([m67](/benchmarks/archive/m67-autotune.md)): converge, retornando o
menor `ef`, com recall médio de 0,986 acima dos alvos. **Ressalvas honestas:** o corpus é fácil — o
baseline com `ef=64` já dá recall 1,0 e todos os alvos convergem para `ef=10`, então a curva de `ef`
não é estressada —; e a cauda mostra 12% de queries fora do alvo, o que significa que o recomendador
é **ótimo na média, não seguro na cauda**.

# Ressalvas

O recall estimado usa ground truth exato amostrado, que é a base honesta, e não um estimador
GT-free. A convergência depende do corpus, com retornos decrescentes — alvos como 0,999 exigem `ef`
super-linear.

[^adr0026]: ADR 0026 — M67 auto-tune de índices
