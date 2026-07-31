---
type: Invariant
title: `pg_total_relation_size` pega lock de relação — sob DDL exclusivo ele BLOQUEIA, e o monitor silencioso parece "nada acontecendo"
description: Medido 2026-07-31: a query de acompanhamento de um ALTER TABLE ... SET LOGGED ficou 163 s em Lock/relation. A saída vazia lê como ausência de atividade, que é o oposto da verdade.
resource: benchmarks/m169_rebuild_heap.sh
tags: [postgres, ddl, lock, monitoramento, instrumento, silencio]
timestamp: 2026-07-31T00:00:00Z
---

# Monitorar o tamanho de uma relação **bloqueia** sob DDL exclusivo

## O caso medido (2026-07-31)

Durante `ALTER TABLE public.hits_heap SET LOGGED` sobre ~70 GB, a query de acompanhamento — que imprimia
`pg_size_pretty(pg_total_relation_size('hits_heap'))` para ver o progresso — apareceu assim em
`pg_stat_activity`:

```
410s  LWLock/WALWrite   ALTER TABLE public.hits_heap SET LOGGED
163s  Lock/relation     (a query de monitoramento)
```

`pg_total_relation_size` precisa **abrir a relação**, e o `ALTER TABLE` segura `AccessExclusiveLock` do início ao
fim. O monitor não é passivo: ele entra na fila do lock.

## Por que custa mais do que parece

O sintoma não é um erro — é **silêncio**. A invocação simplesmente não retorna, o `timeout` do shell a mata, e a
saída vazia se lê como *"não há atividade"*. Foi exatamente essa a leitura errada: uma coleta anterior voltou
vazia e eu a interpretei como "a consulta terminou", quando a verdade era "o meu observador está preso atrás do
trabalho que eu queria observar".

Pior: cada tentativa deixa **mais um backend na fila**. Depois de algumas coletas, o `pg_stat_activity` fica
povoado de clientes bloqueados que nada fazem — e uma guarda de "box ociosa" que conte `client backend` sem olhar
o estado passa a abortar por causa do próprio monitoramento.

## O que usar no lugar

| quero saber | forma que NÃO pega lock |
|---|---|
| o DDL está vivo e progredindo? | `pg_stat_activity` sozinho — `state`, `wait_event_type`, `query_start` |
| quanto já foi escrito? | `df` no filesystem do `data_directory`, ou `du` no diretório da base |
| o passo terminou? | a linha que o próprio script imprime no log ao final do passo |

Ler o log do script é a melhor das três: é o único sinal que fala do *passo*, não de bytes que podem subir por
WAL, por arquivo temporário ou por outra tabela.

## A família

É o instrumento perturbando a medição, como
[`pgrep -f` que casa com o próprio watcher](pgrep-f-casa-com-o-proprio-watcher.md) e
[o VmRSS que inclui shared_buffers](vmrss-de-backend-pg-inclui-shared-buffers.md). A diferença é o sintoma: lá o
instrumento **mente**, aqui ele **emudece** — e silêncio é mais fácil de confundir com informação.

## Relacionados

- [invariant/pgrep-f-casa-com-o-proprio-watcher](pgrep-f-casa-com-o-proprio-watcher.md)
- [invariant/vmrss-de-backend-pg-inclui-shared-buffers](vmrss-de-backend-pg-inclui-shared-buffers.md)
- [failure-mode/contaminacao-por-concorrencia](../failure-modes/contaminacao-por-concorrencia.md)
