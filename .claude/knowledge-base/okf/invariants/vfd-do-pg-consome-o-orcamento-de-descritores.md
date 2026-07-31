---
type: Invariant
title: O VFD do PostgreSQL pode segurar até `max_files_per_process` (1000) dentro de um soft limit de 1024 — uma lib embarcada que abre arquivos fora dele começa com folga quase zero
description: Medido 2026-07-31 no box do M169: max_files_per_process=1000, ulimit -n soft=1024 (hard 1.048.576), 205 GB livres em /tmp. O spill do DataFusion abre arquivos fora do gerenciador de VFD do PG e falhou em File::create com o hint de ulimit — não por disco cheio.
resource: theodb_rs/src/am/df_executor.rs
tags: [postgres, descritores, vfd, datafusion, spill, ffi, limite]
timestamp: 2026-07-31T00:00:00Z
---

# O orçamento de descritores dentro de um backend PostgreSQL

## O fato

O PostgreSQL gerencia seus próprios *virtual file descriptors*. `max_files_per_process` (default **1000**)
autoriza o backend a manter até esse número de arquivos abertos de verdade. O soft limit do processo, medido no
box do M169, é **1024**.

```
max_files_per_process = 1000
ulimit -n (soft)      = 1024        (hard: 1048576)
```

Qualquer biblioteca embarcada que abra arquivos **fora** do gerenciador de VFD — Rust `File::create`, um
`tempfile`, um mmap próprio — não participa dessa contabilidade e disputa a folga que sobra.

## Como isso apareceu

O spill do DataFusion (`GroupedHashAggregateStream` sob pool limitada) cria arquivos de partição via
`tempfile`. Em `datafusion-physical-plan-54.0.0/src/spill/mod.rs:311` a falha de `File::create` é embrulhada
assim:

```rust
exec_datafusion_err!("(Hint: you may increase the file descriptor limit with shell command \
                      'ulimit -n 4096') Failed to create partition file at {path:?}: {e:?}")
```

Duas consequências práticas:

1. **O erro é `DataFusionError::Execution`** — não `ResourcesExhausted`, não `IoError`. Um tratamento que só
   case `ResourcesExhausted` (o caso natural de "faltou memória") **não pega** esta falha.
2. **Não era disco cheio.** Havia 205 GB livres. Ler o hint como "aumente o ulimit" e parar aí perde que o
   consumo vem do VFD do próprio PG.

## O que NÃO está medido (escopo honesto)

Não amostrei `/proc/PID/fd` no instante da falha, então **não sei quantos descritores estavam realmente
abertos**. `max_files_per_process` é **teto, não reserva**: o uso real depende de quantos segmentos de relação
o backend tocou. A evidência sustenta a causa próxima (`File::create` falhou com orçamento apertado e disco
sobrando), não uma contabilidade exata.

Isso importa porque, na mesma corrida, a q32 **também** passa pelo caminho de spill e **completa**, enquanto
q08/q09 (`COUNT(DISTINCT`) falham. Por que uma derrama com sucesso e as outras não é pergunta **aberta** —
provavelmente o número de arquivos que cada operador cria difere, mas isso é hipótese, não medida. Afirmá-la
seria repetir [extrapolar-reta-para-regime-de-outro-mecanismo](../failure-modes/extrapolar-reta-para-regime-de-outro-mecanismo.md).

## A consequência de projeto

Dentro de um backend PG, **spill para disco por uma lib embarcada não é um recurso com o qual se possa contar**
no default. O caminho robusto é tratar a falha de spill como "este plano não completa aqui" e ter uma rota
alternativa, em vez de instruir o operador a subir o `ulimit` — que mascara que o código passou a criar
arquivos temporários que o PG não contabiliza, não limita por `temp_file_limit` e não limpa no crash.

## Relacionados

- [invariant/maintenance-work-mem-nao-capa-rss-de-rust](maintenance-work-mem-nao-capa-rss-de-rust.md) — o irmão: o knob do PG não alcança a alocação feita em Rust
- [measurement/delta-medido-m169-28-para-30](../measurements/delta-medido-m169-28-para-30.md) — a corrida onde q08/q09 regrediram por isto
- [failure-mode/extrapolar-reta-para-regime-de-outro-mecanismo](../failure-modes/extrapolar-reta-para-regime-de-outro-mecanismo.md)
- [invariant/panic-atraves-da-fronteira-c](panic-atraves-da-fronteira-c.md)
