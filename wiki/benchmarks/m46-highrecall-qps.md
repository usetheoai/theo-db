---
type: Measurement
title: m46 — higiene do hot-path: recall-neutro provado, ganho de QPS não estabelecido
description: O controle não modificado derivou massivamente entre as duas corridas, o que invalida qualquer atribuição de QPS — e é o próprio artefato que demonstra isso.
resource: git:f7c7b93:docs/benchmarks/m46-highrecall-qps.md
tags: [benchmark, controle, variancia, honest-negative, metodologia, m46]
milestone: M46
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m46
    resource: git:f7c7b93:docs/benchmarks/m46-highrecall-qps.md
    title: M46 — theodb_hnsw scan hot-path hygiene
---

Veredito em duas partes, e a segunda é a interessante.

# Parte 1 — recall-neutro: PROVADO

As mudanças alteram a **alocação**, não a **ordem de visita**. Provado sobre o binário embarcado: um scan
de índice retornou a ordem **byte-idêntica** à do oráculo exato. Testes unitários codificam a mesma
invariante.

# Parte 2 — ganho de QPS: NÃO ESTABELECIDO

**Honest-negative.** O ambiente de medição estava contendido demais para atribuir qualquer delta de QPS à
mudança, e o regime-alvo não foi reproduzido.

> *"Performance é claim, não opinião. Este relatório NÃO faz claim de superioridade de QPS — a evidência
> não sustenta um."*

# O controle — a verificação de honestidade que carrega o veredito

Este é o mecanismo que vale aprender. O harness mede, **ao lado do índice modificado, a implementação de
referência NÃO modificada**.

**A referência é o controle:** o binário dela é **idêntico** nas duas corridas, então **qualquer variação
no QPS dela é ruído puro da máquina**.

Ela **derivou massivamente** entre as duas corridas — mesma máquina, minutos de diferença.

**Portanto a comparação de QPS está invalidada, e isso é demonstrado, não suposto.** Sem o controle, o
delta observado no índice modificado teria sido reportado como ganho ou perda; com ele, sabe-se que o
instrumento não conseguia medir.

**A próxima medição correta é declarada:** dataset real numa máquina quieta.

# Por que isso importa

Um controle não modificado, medido lado a lado, é a defesa mais barata contra a armadilha que
[m41](/benchmarks/m41-hnsw-qps.md) teve de corrigir depois do fato. Aqui ele evitou o erro **antes** da
publicação.
