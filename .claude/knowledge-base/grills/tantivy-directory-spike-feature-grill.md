---
slug: tantivy-directory-spike
milestone_id: M139
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Grill — M139 SPIKE (o GATE): `Directory` do Tantivy sobre block storage do Postgres

> **Nota de método (honestidade):** as 4 perguntas do grill não foram feitas numa entrevista separada — as
> respostas vêm da análise de fundação conduzida com o owner em 2026-07-21 (peers neon/paradedb/pg_durable, inventário
> medido do repo, e as decisões que ele tomou ao longo dela). Registro a diferença em vez de simular uma
> entrevista que não houve.

**Q1 — O que é e por que AGORA?** Decisão do owner de ter engine lexical própria sobre **Tantivy**
(upstream **MIT**, verificado: `quickwit-oss/tantivy` v0.26.0 — D1 limpo). A pesquisa no ParadeDB mostrou que o
risco NÃO está no BM25: está em fazer o Tantivy viver dentro do Postgres. O `pg_search` tem **105.286 LoC**
(3,2× o TheoDB inteiro) e a parte cara é `MVCCDirectory` (trait `Directory` sobre páginas PG, importando
`pg_sys`), `MvccSatisfies` de 5 modos com `xmin`/`xmax` por segmento, **WAL resource manager próprio**
registrado em `_PG_init`, merges em background worker sob `MergeLock`+advisory, e `ambulkdelete` com barreira
de cleanup-lock. **Por que agora:** é o mesmo método measurement-first que poupou meses no M73 (gap do ScaNN
não-alcançável) e no M74 (RaBitQ dá memória, não QPS). Responder a pergunta difícil em semanas em vez de
descobri-la em trimestres.

**Q2 — Dependências.** M136 `[ ]` — o Postgres com `--enable-cassert` no CI é o que pega violação de invariante
nesta classe de código (é a lição #1 do paradedb, e a classe exata do nosso #143).

**Q3 — Decisões do owner.** "Não importa o esforço" + aceitação do spike como gate (2026-07-21).

**Q4 — Riscos NOVOS.** (a) O ParadeDB **forka** o Tantivy (rev pinada + feature `paradedb`), o que sugere que o
upstream não basta para uso dentro de um banco — se confirmarmos, aciona nossa política D3 de fork
(upstream-first, diff mínimo, CI de rebase, saída quando o upstream alcançar). (b) Um spike pode passar no
caminho feliz e esconder o custo real, que está em VACUUM/merge/paralelismo — por isso o DoD exige crash real
com replay, não só indexar-e-buscar.
