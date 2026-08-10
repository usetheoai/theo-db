---
type: Decision
title: ADR 0049 — Embed assíncrono em 3 fases: liberar o horizonte de xmin durante o HTTP
description: A chamada ao modelo sai de dentro da transação do worker, porque o snapshot ativo prendia backend_xmin — e com ele o horizonte do autovacuum local — por toda a duração do round-trip.
resource: git:f7c7b93:docs/adr/0049-m122-three-phase-async-embed.md
tags: [adr, vectorizer, mvcc, xmin, autovacuum, background-worker, m122]
adr_id: "0049"
adr_status: Accepted
decision_date: 2026-07-20
milestone: M122
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0049
    resource: git:f7c7b93:docs/adr/0049-m122-three-phase-async-embed.md
    title: ADR 0049 — M122 3-phase async embed
    last_modified: 2026-07-20
---

Fecha um risco que o [ADR 0016](/decisions/0016-m54-vectorizer-worker-mechanism.md) havia
antecipado — e o fecha divergindo deliberadamente da referência SOTA.

# O problema

O worker do [vectorizer](/features/16-vectorizer.md) embeda um lote chamando um endpoint HTTP, o que
pode levar até ~90 s sob endpoint travado. Antes desta mudança, o embed rodava **dentro** da
transação do worker.

Como a API de transação do bgworker **empurra um snapshot ativo por todo o corpo do closure**, o
snapshot da transação **prendia o `backend_xmin` durante todo o round-trip HTTP** — criando um piso
no horizonte do autovacuum local pela duração do embed.

# Decisão — três transações de topo

- **Fase A** — uma transação que lê o conteúdo e resolve a configuração de rede (endpoint, modelo,
  chave) em valores Rust próprios, e **commita**.
- **Fase B** — o embed HTTP roda **sem transação, sem SPI e sem leitura de GUC**, portanto **sem
  snapshot ativo**: o `backend_xmin` fica liberado durante toda a chamada.
- **Fase C** — uma transação que escreve os vetores, de forma idempotente por chave primária, e marca
  o job como concluído sob guarda de posse.

A configuração é resolvida na fase A porque ler GUC passa por SPI, que exige transação.

# Alternativas rejeitadas

**O padrão atômico da referência SOTA** — dequeue, embed e write numa transação só, que é o que o
pgai faz **de propósito**. Rejeitado: isso prende o `backend_xmin` através do embed, que é exatamente
o travamento que esta mudança corrige. É aceitável para um worker **externo**; não para um bgworker
**in-process**, cujo `backend_xmin` gateia o autovacuum local diretamente.

**Cachear o embedding em voo por hash de conteúdo** para obter exatamente-uma-vez — fora de escopo
pelo [ADR 0008](/decisions/0008-no-embedding-chat-cache.md); a resposta de recuperação é
re-embedar, o que também é o que a SOTA faz.

# Recuperação de crash — pelo menos uma vez, com escrita idempotente

Um crash depois da fase B mas antes do commit da fase C deixa o job em processamento; a lease expira,
o job é reclamado e re-embedado. A escrita é sobrescrita por chave, portanto idempotente, e a marcação
de conclusão é guardada por posse, de modo que um worker cuja lease expirou não pode marcar um job
já re-reivindicado. **Custo limitado:** um crash desperdiça um re-embed — o trade-off aceito pela
SOTA.

# Consequências

O horizonte de xmin é liberado durante o embed — **medido**: o `backend_xmin` do worker ficou em zero
durante um embed real de 8 s ([m122](/benchmarks/m122-async-embed-xmin.md)).

Custa um commit extra por lote, trivial diante de um HTTP de vários segundos. O modo de chunk-table
**mantém o caminho de transação única** nesta fatia — desvantagem documentada, já que continua
prendendo xmin para vectorizers com chunking. E um GUC de kill-switch permite ao operador reverter ao
comportamento anterior.

## A ressalva mais fina: UPDATE concorrente entre as fases

Se o conteúdo da linha de origem mudar durante a fase B, a fase C escreve o vetor do conteúdo
**obsoleto**. Sob o desenho embarcado de **um worker por banco**, isso se autocorrige: o trigger de
enfileiramento cria um job pendente novo, que o worker embeda em seguida.

**Ressalva prospectiva:** um lançador multi-worker permitiria que um par processando+pendente da mesma
chave fosse reivindicado concorrentemente, e a escrita separada da fase C **alarga a janela de
reordenação** em que o vetor obsoleto poderia chegar por último. O milestone multi-worker terá de
adicionar deduplicação ciente do estado de processamento. **Não é problema no desenho atual.**[^adr0049]

[^adr0049]: ADR 0049 — M122: 3-phase async embed in the vectorizer
