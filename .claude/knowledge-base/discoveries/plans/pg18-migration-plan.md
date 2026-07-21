---
slug: pg18-migration
milestone_id: M135
date: 2026-07-21
owner: paulo
---

# Discovery plan — migração PostgreSQL 17 → 18 de uma extensão com Table AM + Index AM

## Context

A sondagem de compilação (2026-07-21, PG18.4) mediu **27 erros**. 19 são mecânicos; **8 são semânticos**,
concentrados no rework de bitmap scan do PG18. Antes de escrever qualquer linha, queremos saber **como o campo
resolveu exatamente estes problemas** — porque pgvector, pgvectorscale e Citus já atravessaram o 17→18.

## Objective

Produzir um blueprint que responda, com evidência primária, **como portar** cada uma das 5 classes de erro
medidas — e qual a política de versões que projetos comparáveis adotam — de modo que o `/to-plan` seguinte não
precise inventar abordagem.

## In-scope / Out-of-scope

| Projeto | In scope | Out of scope |
|---|---|---|
| `knowledge-base/references/pgvector` | `src/*.c` com guards `PG_VERSION_NUM >= 18` | docs, CI de release |
| `knowledge-base/references/pgvectorscale` | `pgrx` version policy, `Cargo.toml`, código de scan | benchmarks |
| `knowledge-base/references/citus` | tratamento de TableAM/bitmap entre majors | shard rebalancer |
| PostgreSQL upstream | release notes 18 + commits das APIs que quebraram | tudo mais |

## Research questions

| # | Pergunta | Corner | Método |
|---|---|---|---|
| Q1 | Como o pgvector portou o scan para PG18 — quais guards `PG_VERSION_NUM >= 18` e o que mudou dentro deles? | techniques | Read/Grep no clone local |
| Q2 | Qual é o contrato NOVO de `tbm_begin_iterate`/`tbm_iterate` no PG18 e como um TableAM deve iterar agora que `scan_bitmap_next_block` saiu? | techniques | headers PG18.4 + WebSearch/WebFetch do commit upstream |
| Q3 | Como migrar `TupleDescData.attrs` → `compact_attrs`: qual acessor é o correto (`TupleDescAttr` vs `populate_compact_attribute`) e qual o custo/semântica de cada um? | techniques | headers PG18.4 + docs upstream |
| Q4 | Projetos comparáveis mantêm N majors com `#[cfg]`/`#if`, ou cortam versões antigas? Qual a política declarada? | dependencies | pgvector/pgvectorscale/citus + docs |
| Q5 | Qual a política do pgrx para majors (o que `pgrx/pg18` garante e o que NÃO garante)? | dependencies | fonte do pgrx 0.19 + docs |
| Q6 | Como esses projetos TESTAM a migração — o que roda por major (crash, isolamento, regress)? | tests | CI configs dos clones |
| Q7 | Existe teste que force **página lossy** no bitmap (`ntuples < 0`)? Como o campo constrói esse caso? | tests | Grep nos clones + upstream `src/test` |
| Q8 | Que ferramenta detecta drift de API entre majors antes do compilador (se alguma)? | tools | clones + WebSearch |

Budget: 8 perguntas (dentro de 6–14; ≥2 no corner `techniques` conforme `rules/discover-phd-rigor.md` R4).

## Coverage Matrix

| Corner | Perguntas |
|---|---|
| techniques | Q1, Q2, Q3 |
| dependencies | Q4, Q5 |
| tests | Q6, Q7 |
| tools | Q8 |

100% — nenhum corner vazio.

## Acceptance criteria

- Cada técnica com **≥ 2 fontes primárias** (R2) — clone local conta como fonte primária (é o código real).
- **R0 satisfeito**: WebSearch/WebFetch usados e citados para as mudanças upstream do PG18.
- Nenhuma citação fabricada: todo caminho `knowledge-base/references/...` resolve em disco.
- Qualquer afirmação de performance marcada como medida ou `UNBENCHMARKED`.
