---
type: Decision
title: ADR 0039 — ambuild streaming: pico de memória de 4,2× para 1,3× da base, sem FFI
description: O build de 30M passa a caber em 64 GB por streaming de escrita de páginas; o plano previa FFI do tuplesort do Postgres, e a implementação mediu que não era preciso.
resource: git:f7c7b93:docs/adr/0039-m89-ambuild-streaming-verdict.md
tags: [adr, build, memoria, streaming, escala, parsimony, m89]
adr_id: "0039"
adr_status: Accepted
decision_date: 2026-07-12
milestone: M89
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0039
    resource: git:f7c7b93:docs/adr/0039-m89-ambuild-streaming-verdict.md
    title: ADR-0039 — M89 ambuild streaming
    last_modified: 2026-07-12
---

Fecha o teto de memória descoberto no [ADR 0038](/decisions/0038-m88-billion-scale-regime-verdict.md)
— e é o caso mais claro do repositório em que **a solução mais simples venceu o plano por medição**.

# Veredito: critério atingido

Medido a 30M × 128d (15,4 GB de base) numa máquina de 62 GB usáveis
([m89](/benchmarks/m89-ambuild-streaming.md)):

| Build | pico | razão sobre a base | tamanho |
|---|---|---|---|
| v5 f32 | 19,7 GB | **1,28×** | 15 GB |
| v6 sq8 | 23,1 GB | **1,50×** | 4,46 GB |
| build antigo | 64,7 GB | 4,21× | **OOM** |

Ambos completam a 30M na máquina de 64 GB. Toda a suíte verde, zero regressão, e **formato on-disk
inalterado** — sem bump de magic e sem REINDEX, com os testes de scan-igual-a-seqscan provando que o
writer em streaming é byte-correto.

# Como — dois incrementos

1. **Eliminação de clone:** o build **move** o corpus para o índice em vez de cloná-lo; a quantização
   treina a partir do índice por referência.
2. **Escrita de páginas em streaming** (a mudança-chave): os writers recebem posições e vetores **por
   referência** e escrevem cada lista on-the-fly, liberando o blob f32 por lista — o que elimina os
   buffers que copiavam tudo antes do flush.

# O desvio do plano — parsimony justificado por medição

O plano e a entrevista de requisitos haviam escolhido a **FFI do `tuplesort` do Postgres**. **A
implementação não usou FFI**, e a justificativa é medida:

- O primeiro incremento, medido isolado, **ainda estourava a 4,21×** — as cópias dominantes eram o
  clone das entradas de lista (16 GB) e o buffering dos writers (~32 GB), não o clone do build.
- O segundo incremento **atinge o critério com risco muito menor: zero FFI.**

A FFI do `tuplesort` era **YAGNI para o alvo de 30M**. Isto é desvio **parsimony-positivo**: solução
mais simples, critério atingido, menor risco. **Não é workaround** — resolve genuinamente o OOM com
memória limitada por lista e formato byte-idêntico.[^adr0039]

# Limites honestos

**Não é `O(maintenance_work_mem)`.** O pico ainda carrega a cópia 1× do corpus (~15,4 GB a 30M), de
modo que **100M — cerca de 51 GB de base — ainda não cabe em RAM commodity**. O streaming verdadeiro
via `tuplesort`, que nunca materializaria os vetores (heap → sorter → páginas), é o **follow-up
honesto para 100M+**. Este milestone entrega o critério de 30M, não o build em escala de bilhão.

Os formatos antigos, interleaved e não separados, mantêm o caminho anterior e ainda estouram em
escala de bilhão.

# Alternativas consideradas

**FFI do tuplesort agora** — rejeitada para este critério: risco alto, com `unsafe` e virtual slots,
sem necessidade medida. Reservada para quando a cópia 1× virar o gargalo. **Máquina maior para forçar
30M com o build antigo** — mascara a ineficiência acidental em vez de removê-la.

[^adr0039]: ADR-0039 — M89: ambuild streaming (build de memória limitada)
