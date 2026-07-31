---
type: Measurement
title: Materializar o gêmeo heap de 100M custa 1796 s de COPY + 1561 s de SET LOGGED — e o segundo passo não é opcional
description: Medido 2026-07-31 na box de bench. O rewrite de SET LOGGED nunca havia sido cronometrado neste projeto. Custa quase tanto quanto a carga, e um crash horas depois provou que pulá-lo teria apagado as duas coisas.
resource: benchmarks/m169_rebuild_heap.sh
tags: [100m, heap, copy, unlogged, wal, recovery, oraculo]
timestamp: 2026-07-31T00:00:00Z
---

# Materializar o gêmeo heap de 100M: **1796 s + 1561 s**

## Os números

| passo | tempo | resultado |
|---|---|---|
| `\copy` do TSV (69,7 GB) para tabela **UNLOGGED** | **1796 s** (~30 min) | `COPY 99997497` |
| `ALTER TABLE … SET LOGGED` (~66 GB) | **1561 s** (~26 min) | `relpersistence = p` |
| verificação (`count(*)` no heap frio) | ~9 min | 99.997.497 |

Box: 16 vCPU / 31 GB, `/dev/vda1`. Tamanhos finais: `hits` colunar **16 GB**, `hits_heap` **66 GB** — o gêmeo
custa **4,1×** o espaço do colunar para os mesmos dados.

Pico de disco durante o `SET LOGGED`: **247 GB** de 387 GB, caindo para 182 GB quando a cópia antiga foi
liberada. O rewrite precisa do dobro do tamanho da tabela, mais WAL — dimensionar disco pelo tamanho final é
subdimensionar.

## Por que `UNLOGGED` durante a carga e `LOGGED` depois

`UNLOGGED` no `COPY` contém o checkpoint-storm. Mas manter assim é agendar a perda:
[crash recovery trunca toda tabela UNLOGGED](../invariants/unlogged-truncado-por-recovery.md), e este gêmeo é o
oráculo de byte-identidade de duas tasks.

**Isto deixou de ser argumento no mesmo dia.** Horas depois, um `cp` sobre o `.so` mapeado
([invariante](../invariants/cp-sobre-so-mapeado-derruba-o-servidor.md)) derrubou o cluster com SIGSEGV e disparou
crash recovery. As duas tabelas voltaram íntegras — `hits` 99.997.497, `hits_heap` permanente — **porque o
`SET LOGGED` tinha rodado**. Sem ele, aqueles 1796 s de carga teriam ido junto.

O passo que parecia cerimônia custou 26 min e evitou perder 56.

## Consequência para quem for planejar

Reconstruir este oráculo é **~1 hora de máquina**, não "uns minutos". Um plano que trate o gêmeo como
descartável — ou um script que o dropé antes de provar que consegue recarregá-lo, ver
[destruir antes de provar a precondição](../failure-modes/destruir-antes-de-provar-a-precondicao.md) — está
apostando uma hora por execução.

## Relacionados

- [invariant/unlogged-truncado-por-recovery](../invariants/unlogged-truncado-por-recovery.md)
- [invariant/cp-sobre-so-mapeado-derruba-o-servidor](../invariants/cp-sobre-so-mapeado-derruba-o-servidor.md) — o crash que testou a decisão
- [failure-mode/destruir-antes-de-provar-a-precondicao](../failure-modes/destruir-antes-de-provar-a-precondicao.md)
- [measurement/count-star-colunar-100m-com-e-sem-pushdown](count-star-colunar-100m-com-e-sem-pushdown.md)
