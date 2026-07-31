---
type: Measurement
title: Offsets i32 do Arrow estouram acima de 21,5 B/linha sobre 100M
description: DataType::Utf8 usa offsets i32 (2 GB por array); TRÊS consultas do ClickBench estouram (q20, q33, q34), todas sobre URL, com ordens de grandeza de folga — e é panic, não Result.
resource: docs/benchmarks/m162-artifacts/theodb-100m-partial.jsonl
tags: [arrow, escala, colunar]
timestamp: 2026-07-30T00:00:00Z
---

# Offsets `i32` do Arrow estouram acima de 21,5 B/linha sobre 100M

## A conta

`DataType::Utf8` usa offsets `i32` → teto de `i32::MAX = 2.147.483.647` bytes cumulativos por array. Sobre
99.997.497 linhas isso são **21,5 bytes/linha em média**. Qualquer corpus de URLs está ordens de grandeza acima.

## A observação (não é inferência)

```
docs/benchmarks/m162-artifacts/theodb-100m-partial.jsonl:21
{"q": 20, "node": "PUSHDOWN:Custom Scan (theodb_columnar_agg)", "cold_s": 92.199,
 "hot_s": null, "status": "ERR:byte array offset overflow"}
```

Registrado em 2026-07-26 sobre o q20. O baseline COMPLETO do M169 (2026-07-30, 43/43, box de 31 GB) mostra que
o defeito tem **três instâncias**, não uma — e as três com `agg_routed=true`, isto é, dentro do caminho que o
milestone toca:

| q | veredito | tempo até estourar |
|---|---|---|
| q20 | `error:XX000 byte array offset overflow` | 52,1 s |
| q33 | idem | 57,3 s |
| q34 | idem | 48,4 s |

As três agregam sobre a **mesma coluna** (`URL`) — evidência de causa única, não de três bugs distintos, e
consistente com a conta acima: `URL` é a única coluna do corpus larga o bastante para passar de 21,5 B/linha.
Fonte: `docs/benchmarks/m169-baseline-100m.md`.

Consequência para o fix: decodificar por chunk-group deve fechar as três de uma vez. Mas **o modo de falha pode
mudar em vez de sumir** — removido o teto de offsets, o que resta é o pico de memória do ×3 descrito abaixo, que
pode reaparecer como `timeout` ou OOM. Um `ok` nas três é o gate; qualquer outro veredito é resultado parcial,
não sucesso.

O que é leitura de código é apenas a **cadeia interna** até
`arrow-array 58.3.0` (pinada em `Cargo.lock`) `src/builder/generic_bytes_builder.rs:87` — a citação é sensível à versão: na 54.3.1 a mesma linha é a 86.

## Duas propriedades que agravam

1. **É `panic!`, não `Result`** — `.expect("byte array offset overflow")`. Quando a mensagem aparece, a memória
   já foi toda alocada: o panic é o fim de um caminho que **já pagou o pico**.
2. **Nada evita o decode** — predicado de texto nunca dirige skip de zone-map (`df_executor.rs:466-467`, só
   `predicates` chega ao `decode_columns_v2`), então as ~100M células de `URL` são decodificadas sempre.

## O multiplicador que o enunciado não mencionava

`df_executor.rs:305` faz `String::from_utf8_lossy(b).into_owned()` **por célula** — a 100M são ~100M `String`
vivas ao mesmo tempo que os `Option<Vec<u8>>` de origem e a array Arrow em construção: **três cópias
coexistentes**. O teto `i32` é o sintoma que grita; o ×3 de memória é o que mata a corrida.

## Teto residual do fix por chunk-group

Decodificar por chunk-group **desloca** o teto, não o elimina: `2^31 / 10.000` = **214.748 B/célula**. Três ordens
de grandeza de folga para URLs.
