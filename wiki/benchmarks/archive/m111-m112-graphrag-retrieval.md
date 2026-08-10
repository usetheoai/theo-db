---
type: Measurement
title: m111/m112 — GraphRAG em dataset real: o vetor vence em TODAS as configurações
description: O veredito mais desconfortável do repositório — o pilar de grafo, construído com gate medido e engenharia validada, não bate um recuperador denso na tarefa de recuperação.
resource: git:f7c7b93:docs/benchmarks/archive/m111-m112-graphrag-retrieval.md
tags: [benchmark, graphrag, hotpotqa, honest-negative, recuperacao, arquivo, m111]
dataset: HotpotQA
milestone: M111+M112
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m111
    resource: git:f7c7b93:docs/benchmarks/archive/m111-m112-graphrag-retrieval.md
    title: M111/M112 — GraphRAG retrieval on real HotpotQA
    last_modified: 2026-07-17
---

**O veredito medido: o vetor vence em TODAS as configurações.**

# Por que este artefato é o mais duro do repositório

O pilar de grafo passou por **um gate medido** ([m107](/benchmarks/m107-graph-spike.md), 106–232× na
primitiva), **persistência validada** ([m108](/benchmarks/archive/m108-persisted-csr.md)),
**vetorização com oráculo por lane** ([m109](/benchmarks/m109-msbfs.md)) e **extração provada idêntica**
([m110](/benchmarks/archive/m110-extraction.md)).

**Toda a engenharia estava certa. E na tarefa que motivava o pilar — recuperação multi-hop — um
recuperador denso comum vence.**

# O método, que não dá desculpa

Dataset real de perguntas multi-hop, o mesmo usado pela literatura da área. **Embeddings reais e extração
por LLM real.** Baseline: um embedder denso forte. Métrica: recall dos parágrafos de suporte, por
pergunta.

Nenhuma das saídas fáceis está disponível — não é corpus sintético, não é baseline fraco, não é métrica
escolhida a dedo.

# A separação que o ADR já havia feito

O [ADR 0048](/decisions/0048-m107-native-graph-engine-go.md) declarara, ao abrir o pilar, que **a
qualidade do grafo é avaliação separada, que o motor não resolve**.

**Essa separação foi o que permitiu ao pilar ser avaliado honestamente:** o gate do motor mediu o motor e
passou; a avaliação da tarefa mediu a tarefa e falhou. Se os dois tivessem sido confundidos num gate só,
ou o motor teria sido rejeitado por uma razão errada, ou a falha de recuperação teria sido encoberta pelo
sucesso da travessia.

**O motor continua sendo 106–232× mais rápido na primitiva.** O que a medição diz é que **a primitiva
mais rápida não torna a abordagem melhor para esta tarefa** — e publicar isso, tendo construído o pilar,
é o teste real da disciplina que o repositório inteiro afirma seguir.
