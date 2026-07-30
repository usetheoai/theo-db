---
type: Technique
title: Todo log de medição carrega a identidade do binário e da máquina
description: Sem a identidade do binário e da máquina no cabeçalho, um artefato não é evidência — é um número solto. O exemplar do repo grava 2 dos 5 campos: dívida declarada, não regra cumprida.
resource: benchmarks/m168_collect_all.sh
tags: [benchmark, artefato, rigor]
timestamp: 2026-07-30T00:00:00Z
---

# Todo log de medição carrega a identidade do binário e da máquina

## O defeito que ensinou — M168

Uma revisão encontrou o verdict citando um `so_md5` que **nenhum artefato carregava**, com a tabela de memória
tirada de um binário e a de throughput de outro (mais antigo, anterior ao fail-open), e três dos cinco logs sem
cabeçalho algum — enquanto o documento mandava "ver o cabeçalho de cada artefato". Não havia binário único sobre
o qual todos os números publicados tivessem sido tomados.

## O padrão

Um coletor único, que grava o mesmo cabeçalho em **todos** os logs de uma passada:

```bash
so_md5=$(md5sum "$SO_PATH" | cut -d' ' -f1)
[ -z "$so_md5" ] && { echo "FATAL: sem proveniência não é evidência"; exit 2; }
echo "so_md5=$so_md5"; echo "so_path=$SO_PATH"; echo "postmaster=$PM"
echo "nproc=$(nproc) free_g=$(free -g|awk '/^Mem:/{print $2}') loadavg=$(cut -d' ' -f1-3 /proc/loadavg)"
```

E, no M169, dois campos a mais que se provaram decisivos: `maintenance_work_mem` e `shared_buffers` — porque com
`mwm=2GB` o milestone é literalmente **inmedível**, então a configuração faz parte da evidência, não do ambiente.

## Regra derivada

O `so_md5` também é o guard contra o
[invariant/so-obsoleto-sob-shared-preload](../invariants/so-obsoleto-sob-shared-preload.md): se o log traz o md5
do arquivo em disco mas o postmaster mapeou o antigo, comparar `postmaster` (start time) com a data do build
denuncia.

## Estado real do exemplar — dívida declarada

> **CORRIGIDO 2026-07-30 (round 3).** Este conceito prescrevia cinco campos e o irmão
> `cobertura-alegada-sem-execucao` afirmava que "**todo** artefato de medição carrega" os cinco. Medido:
> `benchmarks/m168_collect_all.sh` grava **`so_md5` e `postmaster`** — `grep` por `nproc`, `free` e `loadavg`
> devolve **zero**. O único script que emite `loadavg` é `m168_drift_control.sh`, e esse não emite
> `postmaster`/`nproc`/`free`.
>
> Os cinco campos continuam sendo a regra — cada um serve a um confundidor distinto e o M169 provou que
> `maintenance_work_mem` e `shared_buffers` também precisam entrar. Mas **nenhum script do repo os grava todos**,
> e dizer que grava era a `cobertura-alegada-sem-execucao` aplicada à própria Technique.

## Relacionados

- [invariant/so-obsoleto-sob-shared-preload](../invariants/so-obsoleto-sob-shared-preload.md)
