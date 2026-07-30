---
type: Index
title: Negativos honestos
description: Índice dos conceitos do tipo `Honest Negative` deste bundle.
tags: [okf, indice]
timestamp: 2026-07-30T00:00:00Z
---

# Negativos honestos

Apostas que foram **medidas e refutadas**. Existem para que a mesma ideia não volte a cada planejamento
parecendo novidade — cada uma custou um ciclo de implementar-medir-reverter.

O mandato do projeto é explícito: *nunca mascare números*. Um milestone que produz zero é resultado, e o registro
dele é o que impede o custo de repetição.

| Conceito | O que é |
|---|---|
| [Uma perna BM25 9,8× mais forte NÃO vence na fusão RRF](bm25-na-fusao-rrf.md) | O RRF premia complementaridade, não força individual — trocar a perna lexical por uma muito melhor não melhorou a fusão. |
| [MIN/MAX sobre texto não é roteável: byte-min ≠ collation-min](min-max-texto-e-colacao.md) | Determinismo de colação não basta — ordenar não é o mesmo que igualdade. Só C/POSIX é seguro, e as colunas default do ClickBench declinam. |
| [pg_duckdb force_execution sobre heap é 0,63-0,89× do row-executor do PostgreSQL](pgduckdb-sobre-heap-e-mais-lento.md) | Resultados corretos e plano usando DuckDB — e ainda assim mais lento que o executor nativo, em todas as escalas. |
| [DoD de ≤1,2× vs pgvector FALSIFICADO — page-native é 7-23× mais lento](resume-from-discarded-m118.md) | O caminho page-native não alcança o alvo; o own-path fica em ~1,95× a recall 1.0. Registrado como ADR-0033. |
| [SBQ não ganha QPS em regime algum — nem in-RAM, nem sob pressão de memória](sbq-nao-ganha-qps-em-regime-algum.md) | A tese de ≥2× foi falsificada em TODOS os regimes medidos (0,35-0,77×), e o mecanismo é conhecido — o HNSW tem localidade de acesso, então o índice f32 não thrasha sob pressão. |
| [Spherical k-means é no-op provado para distância cosseno](spherical-kmeans-para-cosine.md) | Implementado, medido, revertido: para cosine a normalização já ocorre, e o k-means esférico não muda nada. |
| [Superioridade de QPS vetorial sobre ScaNN/AlloyDB é NÃO-ALCANÇÁVEL por extensão PG permissiva](superioridade-vetorial-vs-scann.md) | Veredito medido do M73: o gap de 25-44× a recall 0.99 é de paradigma (AH-LUT anisotrópico + não pagar o imposto MVCC/WAL), não de otimização. |
| [SymphonyQG in-PG: AM correto, gate não atingido](symqg-in-pg.md) | Off-PG o 1-bit co-locado dava paridade + 1,8-2,66×; dentro do PostgreSQL o hnsw continua 2,6-3,9× mais rápido em warm. |
| [Top-N colunar: cobertura ZERO, não rotear](topn-columnar.md) | O PostgreSQL já usa top-N heapsort (equivalente ao TopK do DataFusion); Sort não é o gargalo — o custo é materialização. |
