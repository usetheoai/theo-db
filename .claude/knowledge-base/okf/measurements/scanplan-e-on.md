---
type: Measurement
title: O plano do scan colunar é O(N) — 48,1 MiB a 100M, para QUALQUER projeção
description: plan_columnar_scan desserializa a grade inteira do diretório (n_chunk_groups × natts), não a largura da projeção — então uma consulta de 1 coluna paga o mesmo que SELECT *.
resource: theodb_rs/src/am/columnar.rs
tags: [colunar, escala, memoria]
timestamp: 2026-07-30T00:00:00Z
---

# O plano do scan colunar é O(N) — 48,1 MiB a 100M, para **qualquer** projeção

## A medição

`columnar.rs:1068`:

```rust
let entries = codec::deserialize_directory(&dir_bytes, header.n_chunk_groups as usize * natts)?;
```

**`natts`**, não a largura da projeção. A 100M com os 105 atributos do `hits`:

| | |
|---|---|
| chunk-groups | 10.000 |
| × colunas | 105 |
| entries | **1.050.000** |
| `ChunkDirEntry` em memória | 48 B (2×`u64` + 6×`u32` + 3×`bool`, align 8) |
| **total** | **48,1 MiB**, alocados **antes** do primeiro batch, **fora** da `MemoryPool` |

## Por que importa mais do que parece

Uma tabela numa revisão minha afirmava "projeção estreita (5 col) → 2,3 MiB". **É falso** — e falso justamente
para o q20, que projeta **uma** coluna e é a métrica única do milestone. Todo scan paga a grade inteira.

A 1M são 0,5 MiB, e é por isso que nenhum milestone anterior o pegou.

## Estado

Não é fatal, mas: (a) é alocado antes de qualquer entrega, sem sobreposição com o processamento; (b) sai **fora**
da `MemoryPool`, invisível à contrapressão e ao spill; (c) o milestone publicaria "decode O(k)" com um termo O(N)
não declarado.

**Decisão: medir e declarar, não redesenhar** (degrau 1 da parsimony ladder). Se a medição mostrar que domina o
pico, abre milestone próprio para diretório lazy por stripe.

## Nuance de tamanho

O doc-comment diz "fixed 44 bytes" — é o **serializado**. Em memória são **48 B**. Os dois estão certos para
coisas diferentes.
