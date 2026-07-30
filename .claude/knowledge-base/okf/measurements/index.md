---
type: Index
title: Medições
description: Índice dos conceitos do tipo `Measurement` deste bundle.
tags: [okf, indice]
timestamp: 2026-07-30T00:00:00Z
---

# Medições

Números que **já foram medidos**, com a metodologia e o artefato que os sustentam. Consultar antes de medir de
novo — e antes de afirmar qualquer coisa que um destes já responde.

Toda entrada aqui distingue o que foi **observado** do que foi **calculado**, e marca `UNBENCHMARKED` o que
continua hipótese.

| Conceito | O que é |
|---|---|
| [flush_pending consome ≈ maintenance_work_mem × 8](amplificacao-maintenance-work-mem.md) | Medido por OOM real: mwm=2GB produziu 23,4 GB de anon-rss; mwm=128MB completou a carga de 100M. A fórmula dá a ordem de grandeza e SUBESTIMA o observado em 36% (base: previsto, unidades uniformizadas em GiB). |
| [O mesmo binário deu −0,6% e +2,3% em coletas diferentes da mesma box](deriva-de-box-m168.md) | Controle de deriva do M168: reconstruir o binário antigo e rodá-lo intercalado com o novo fechou a pergunta por experimento — a diferença entre coletas era da box. |
| [Gap medido vs ClickHouse no ClickBench: 19,4× geral, 7,54× na classe coberta, 303× na não-coberta](gap-vs-clickhouse-m159.md) | Mesma box. O landscape publicado situa o resultado, e o deep-dive identificou a ponte de decode como gargalo da classe coberta. |
| [O júri adversarial descartou 11 de 18 achados — precision 0.39](juri-adversarial-precision-039.md) | Medida da precisão de um review multi-agente: a maioria dos descartes era convenção deliberada lida como defeito. |
| [A 100M o modo de falha deixa de ser lentidão e vira NÃO-CONCLUSÃO: 19/43 consultas completam](limite-de-escala-100m-19-de-43.md) | O ClickHouse serve as 43 em 0,008-10,1 s no mesmo box; o TheoDB completa 19, com 5 falhas duras. A taxa de conclusão é o veredito, não a razão. |
| [O pushdown agregado NÃO é a regressão de memória do q17 a 100M](q17-pushdown-nao-e-regressao.md) | Isolado em box ociosa, com RSS do backend amostrado durante: 4,58 GB com pushdown ON vs 4,57 GB OFF. O OOM de 12,3 GB vinha do oráculo do harness. |
| [O plano do scan colunar é O(N) — 48,1 MiB a 100M, para QUALQUER projeção](scanplan-e-on.md) | plan_columnar_scan desserializa a grade inteira do diretório (n_chunk_groups × natts), não a largura da projeção — então uma consulta de 1 coluna paga o mesmo que SELECT *. |
| [Offsets i32 do Arrow estouram acima de 21,5 B/linha sobre 100M](teto-offsets-i32.md) | DataType::Utf8 usa offsets i32 (2 GB por array); o q20 do ClickBench estoura com ordens de grandeza de folga — e é panic, não Result. |
| [Há TRÊS contadores chamados cobertura no ClickBench, e eles não se contradizem](tres-contadores-de-cobertura-clickbench.md) | 35/43 (pushdown sob GUC default), 31/43 (só agg) e os intermediários históricos medem coisas diferentes — confundi-los é a leitura errada mais provável. |
