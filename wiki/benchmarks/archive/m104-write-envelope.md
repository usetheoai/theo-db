---
type: Measurement
title: m104 — memória de escrita colunar limitada: o sinal é contagem de stripes
description: Escolhe como evidência um sinal determinístico e estrutural — quantos stripes a escrita produz — em vez de medir memória, que seria ruidosa.
resource: git:f7c7b93:docs/benchmarks/archive/m104-write-envelope.md
tags: [benchmark, memoria, escrita, sinal-deterministico, arquivo, m104]
milestone: M104
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m104
    resource: git:f7c7b93:docs/benchmarks/archive/m104-write-envelope.md
    title: M104 — bounded columnar write memory
    last_modified: 2026-07-16
---

# O que é medido

O flush incremental de stripe: uma escrita colunar descarrega um stripe assim que os bytes pendentes
excedem a memória de manutenção configurada, de modo que o pico de escrita seja **proporcional à
configuração, e não ao número de linhas da transação**.

# O sinal escolhido, e por que ele é bom

**A evidência é a linearidade da contagem de stripes**, não uma medição de memória:

> se a memória fosse proporcional ao número de linhas, **uma única inserção bufferizaria tudo e produziria
> UM stripe**; com o limite, ela produz N stripes, e o conjunto pendente **nunca excede** a configuração.

**Isso é determinístico e estrutural.** Contar stripes não tem variância; medir pico de RSS numa máquina
compartilhada tem — e vários artefatos desta linhagem foram invalidados exatamente por isso.

**Escolher um sinal que o comportamento sob teste implica logicamente**, em vez de um proxy ruidoso, é o
que torna a prova barata e sólida ao mesmo tempo.

# Contexto

É um dos bounds que o [ADR 0047](/decisions/0047-m104-scaling-tradeoffs-deliberate.md) instalou —
junto com o teto do cache, o chunking do batch de IA e a backpressure da fila —, e que permitiu àquele
ADR afirmar que **nenhuma trilha de memória ilimitada permanece sem sinalização**.
