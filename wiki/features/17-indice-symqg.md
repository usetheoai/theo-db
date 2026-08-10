---
type: Feature
title: Índice SymphonyQG (theodb_symqg) — experimental, medido mais lento
description: Grafo quantizado co-localizado em clean-room; o veredito medido é que o HNSW próprio é 2,6–3,9× mais rápido a recall casado, então este não é o default recomendado.
resource: git:f7c7b93:docs/features/17-indice-symqg.md
tags: [feature, indice, symqg, experimental, honest-negative, quantizacao]
feature_status: experimental — não recomendado como default
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat17
    resource: git:f7c7b93:docs/features/17-indice-symqg.md
    title: Criar um índice vetorial SymphonyQG
---

**Status: entregue como access method alternativo e experimental.** É um grafo quantizado
co-localizado, implementado em **clean-room** a partir do paper — a referência em C++ foi **estudo
apenas, nunca copiada**, por causa do portão de licença.

# A honestidade crítica: não use como default

**O veredito medido é que este índice é MAIS LENTO que o [HNSW](/features/02-indice-hnsw.md) próprio
dentro do PostgreSQL.** Em SIFT1M, com recall casado, o HNSW é **2,6–3,9× mais rápido** na faixa
prática de recall entre 0,95 e 0,994 — **o gate de superioridade não foi atingido**
([verdict](/benchmarks/e2-symqg-inpg-verdict.md)).

O kernel FastScan de 1 bit dá ganho **modesto**, de 1,07 a 1,22×, sobre o mesmo índice
([ablação](/benchmarks/e2-symqg-fastscan-verdict.md)).

E há uma lição de método aqui: o **spike fora do PostgreSQL** chegou a 1,8–2,66× sobre a referência
([spike](/benchmarks/e2-symqg-spike.md)), **e isso não transferiu para dentro do banco**. É o mesmo
padrão do [ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md): ganho medido in-memory
frequentemente não sobrevive ao imposto de página, WAL e MVCC.

**Use o `theodb_hnsw` como default vetorial.** Nenhuma promessa de superioridade de latência é feita
aqui.

# Superfície

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

CREATE INDEX itens_symqg
ON itens
USING theodb_symqg (embedding theodb_symqg_l2_ops)
WITH (degree_bound = 32);
```

**Somente L2.** A opclass default é a única — um build com métrica diferente **falha rápido**, em vez
de silenciosamente indexar com a métrica errada.

O kernel FastScan é controlado por GUC, ligado por padrão, e o kill-switch existe justamente porque o
ganho dele é modesto e pode não compensar em todo regime.

# Por que continua no código

Pelo mesmo racional do [ADR 0018](/decisions/0018-m57-sbq-inline-not-superior.md): um formato
versionado e correto não custa manutenção ativa e é base de experimentação futura. O que ele **não** é
é caminho de performance — e não deve embasar claim nenhum.
