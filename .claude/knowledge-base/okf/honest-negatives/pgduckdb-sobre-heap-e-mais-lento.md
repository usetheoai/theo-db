---
type: Honest Negative
title: pg_duckdb force_execution sobre heap é 0,63-0,89× do row-executor do PostgreSQL
description: Resultados corretos e plano usando DuckDB — e ainda assim mais lento que o executor nativo, em todas as escalas.
tags: [htap, duckdb, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# `pg_duckdb` `force_execution` sobre heap é **0,63-0,89×** do row-executor do PostgreSQL

> **CORRIGIDO 2026-07-30 após review.** A versão anterior publicava **0,52-0,78×** — uma faixa que **não existe
> em artefato algum** deste projeto. A medição real está abaixo. A conclusão qualitativa não muda; os dois
> extremos publicados eram fabricados, e estavam no título.

## O veredito — medido em 3 escalas × 3 runs (`m61-columnar-adoption.md:26-28`)

| escala | PG row-executor (ms) | DuckDB heap (ms) | razão de velocidade |
|---|---|---|---|
| 100k | 23,6 ± 5,2 | 26,4 ± 1,9 | **0,89×** |
| 1M | 108,4 ± 15,0 | 164,2 ± 5,3 | **0,66×** |
| 5M | 394,4 ± 12,2 | 627,8 ± 111,2 | **0,63×** |

O DuckDB gasta **mais** milissegundos em todas as escalas; a razão é de **velocidade** (`23,6/26,4 = 0,89`),
não de tempo.

> **SEGUNDA correção, 2026-07-30 (re-review).** Ao substituir a faixa fabricada pela medida, eu **inverti as
> colunas** — publiquei 23,6 ms como DuckDB e 26,4 ms como PG, o que faria o DuckDB parecer **mais rápido** e
> contradiria o próprio título. É a mesma espécie de defeito ("rótulos trocados") que eu havia imputado ao
> original. A citação de linha estava certa; a transcrição não.

Forçar o DuckDB a executar sobre o **heap** do PostgreSQL é **mais lento que o executor nativo** em **todas** as
escalas. E — o padrão que a faixa sozinha esconde — **piora conforme a escala cresce**, que é o oposto do que
"a vantagem do DuckDB é do formato" faria supor a quem só lê os extremos. O resultado não é artefato de
configuração:

- resultados **corretos** (match ✓);
- o plano de fato usa DuckDB (`duckdb_plan=True`).

## A explicação, e por que ela generaliza

Um motor vetorizado só ganha quando lê **dados no layout dele**. Sobre um heap row-major, ele paga a conversão
linha→coluna e não colhe nenhuma das vantagens (compressão, skip por zone-map, leitura de coluna isolada). A
vantagem do DuckDB é **do formato**, não do executor.

Isso é o que sustenta a decisão de investir no `theodb_columnar` own-code em vez de terceirizar execução: sem
storage colunar próprio, o motor vetorizado não tem do que se alimentar.

## Consequência histórica

Junto com a licença e o peso (118 MB de bundle C++ estático), este número embasou o tier-out do `pg_duckdb` para
imagem opcional (M142) e depois a remoção total (M143).

## Relacionados

- [honest-negative/topn-columnar](topn-columnar.md)
