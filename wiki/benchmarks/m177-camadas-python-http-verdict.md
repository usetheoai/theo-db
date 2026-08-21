---
type: Measurement
title: m177 — o servidor em Python e o HTTP custam 5,1% juntos, e eliminar o TCP rende 0,2%
description: Decompõe o pedido por camada e mostra que o teto de qualquer otimização de transporte é 0,85 ms sobre 16,6 ms; o Unix socket ganha do TCP com significância e sem relevância prática.
resource: git:7cd157d^:benchmarks/artifacts/m177/layers.json
tags: [benchmark, m177, embedding, python, http, unix-socket, transporte, teto-de-otimizacao, significancia]
milestone: M177
generated: { by: claude-code/opus-5, at: 2026-08-07T23:30:00Z }
sources:
  - id: layers
    resource: git:7cd157d^:benchmarks/artifacts/m177/layers.json
    title: Decomposição por camada — inferência pura, HTTP/TCP, HTTP/UDS
  - id: udssig
    resource: benchmarks/m177_uds_significance.py
    title: TCP loopback vs Unix domain socket, pareado, sem modelo no laço
---

Responde duas perguntas de otimização com número, em vez de intuição: **quanto se perde por o servidor
ser em Python** e **quanto se perde por o transporte ser HTTP** — incluindo se há técnica para eliminar
o segundo.

# A decomposição

Mesmo modelo (`bge-small-en-v1.5`), mesmo texto, conexão reutilizada, n=60 por camada:

| camada | p50 | média ± dp | o que inclui |
|---|---|---|---|
| **A — in-process** | **15,800 ms** | 16,706 ± 2,195 | só `InferenceSession.run` (ONNX, C++) |
| **C — HTTP sobre TCP** | **16,649 ms** | 18,630 ± 5,483 | A + servidor Python + HTTP + TCP loopback |
| D — HTTP sobre UDS | 18,624 ms | 22,285 ± 7,999 | A + servidor Python + HTTP + Unix socket |

**O custo de ter servidor em Python + HTTP + TCP é 0,849 ms — 5,1% do pedido.**

Isso é o **teto** de qualquer otimização de transporte, incluindo embarcar o modelo no PostgreSQL: mesmo
eliminando tudo — servidor, protocolo, socket — não se recupera mais que esses 5,1%, porque os outros
94,9% são o ONNX Runtime, que já é código nativo e continuaria sendo executado igual.

Casa com o [flamegraph](/benchmarks/m177-embed-concurrency-verdict.md): 98,6% das amostras em
`InferenceSession.run`, ~1,4% em HTTP+JSON+tokenização.

# O Unix socket: significativo e irrelevante, medido separadamente

A camada D acima saiu **mais lenta** que a C, o que contraria a teoria — socket Unix não paga
handshake, checksum nem roteamento. A causa é a variância do modelo (dp de 8,0 ms) afogando uma
diferença de microssegundos. Re-medido **sem modelo no laço**, alternado e pareado, n=400:

| | round-trip |
|---|---|
| TCP loopback | 0,463 ± 0,088 ms |
| **Unix domain socket** | **0,431 ± 0,081 ms** |
| delta pareado | **0,033 ms** · ci95 [0,026 – 0,039] · **p = 0,0000** |

**O Unix socket é de fato mais rápido, com significância estatística inequívoca — e ganha 33
microssegundos.** Sobre um pedido de 16,6 ms, isso é **0,2%**.

Este é o caso didático de **significância estatística sem relevância prática**. Com n=400 e ruído baixo,
o bootstrap detecta uma diferença que nenhum usuário perceberia. Reportar apenas "p=0,0000, o Unix
socket vence" seria verdadeiro e enganoso.

# Técnicas para eliminar o HTTP, e o que cada uma pode render

| técnica | ganho máximo teórico | medido | veredito |
|---|---|---|---|
| Unix domain socket | ≤ 0,85 ms | **0,033 ms (0,2%)** | não paga a mudança |
| protocolo binário no lugar de JSON | ≤ 0,85 ms | não medido | disputa a mesma fatia de 5,1% |
| memória compartilhada | ≤ 0,85 ms | não medido | idem, com complexidade muito maior |
| **modelo embarcado no backend** | ≤ 0,85 ms | não medido | idem — e custa [1,7 GB por backend](/benchmarks/m177-hop-vs-residencia-verdict.md) |

**Todas disputam a mesma fatia de 5,1%.** É a razão pela qual a discussão de transporte é menos
importante do que parecia: o limite superior é conhecido, e é pequeno.

Para comparação, as alavancas **fora** do transporte já medidas neste milestone:

| alavanca | ganho medido |
|---|---|
| não estrangular o servidor com `OMP_NUM_THREADS=1` | **9,4×** |
| trocar `e5-large` por `MiniLM-multilingual` | **3,7×** (com qualidade ainda não medida) |
| eliminar todo o transporte | **1,05×** |

# O que "servidor em Python" custa de verdade

**Menos do que a intuição sugere, e por um motivo estrutural:** o Python não executa a inferência. Ele
recebe bytes, chama o ONNX Runtime — que é C++ e **libera o GIL** durante a execução — e serializa o
retorno. O trabalho pesado nunca esteve em Python.

Reescrever o servidor em Rust atacaria uma fração dos 5,1%, e apenas a fração que não é o parser HTTP do
sistema. **A escada de parcimônia resolve isto no degrau 1**: a reescrita não precisa existir.

Uma ressalva honesta: isto vale para o regime medido — **um texto por pedido**. Em lotes grandes a
serialização JSON de milhares de vetores cresce, e a proporção pode mudar. **Não medido.**

# Limites honestos

- **Uma máquina, não dedicada** (12 cores, 15 GB, dez containers ativos), um modelo (384d), um texto por
  pedido. Os 5,1% são desta configuração.
- A camada D da primeira tabela está **contaminada pela variância do modelo** — é por isso que o
  veredito sobre UDS vem da segunda medição, sem modelo, e não dela.
- **Não foi medido** protocolo binário, memória compartilhada, nem o comportamento em lote grande.
- O ganho de 5,1% é o teto do transporte **em latência**. Não diz nada sobre throughput sob
  concorrência, medido separadamente ([~61 rps](/benchmarks/m177-embed-concurrency-verdict.md)).

# Relacionados

- O flamegraph que apontou os 98,6%: [concorrência e perfil](/benchmarks/m177-embed-concurrency-verdict.md)
- Custo e residência do modelo: [fase 1](/benchmarks/m177-hop-vs-residencia-verdict.md)
- O prior art de embarcar: [prior art](/references/embedding-local-como-extensao-2026-08.md)
- O desenho atual: [embeddings em SQL](/guides/sql-embeddings.md)
