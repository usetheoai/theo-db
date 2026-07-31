---
type: Invariant
title: `pgrep -f <padrão>` casa com a linha de comando do PRÓPRIO watcher — o laço de espera nunca termina
description: `until ! pgrep -f "cargo build"; do sleep 20; done` roda para sempre, porque o shell que executa o laço tem "cargo build" no próprio argv. O build terminou em 2m11s e o watcher continuou reportando RODANDO.
resource: .claude/knowledge-base/okf/log.md
tags: [plataforma, shell, espera, instrumento, falso-positivo]
timestamp: 2026-07-31T00:00:00Z
---

# `pgrep -f` casa com o **próprio** watcher

## O caso medido (2026-07-31)

Para esperar o fim de um build remoto:

```bash
ssh box 'until ! pgrep -f "cargo build" >/dev/null; do sleep 20; done; echo TERMINOU'
```

O build terminou (`Finished release profile in 2m 11s`, 0 erros) e o laço **continuou girando**. A verificação
manual reportava `build: RODANDO` com o log já fechado.

`pgrep -f` casa contra a **linha de comando inteira** de todo processo — inclusive a do shell que executa o laço,
cujo `argv` contém a string `cargo build` porque ela está no próprio comando. O watcher se via, concluía que o
alvo estava vivo, e esperava por si mesmo indefinidamente.

## Por que este engana mais que o normal

O sintoma é **ausência**: nada falha, nada loga erro, o exit code nunca chega. Uma espera que não termina parece
"o trabalho ainda está rodando" — a leitura mais plausível e a mais cara, porque leva a esperar mais em vez de
investigar. E o falso positivo é **100% reprodutível**, então tentar de novo confirma a conclusão errada.

## As saídas

| forma | por quê |
|---|---|
| `pgrep -f "[c]argo build"` | a classe de caractere não casa com o literal do próprio argv — o truque clássico do `ps \| grep` |
| `pgrep -x cargo` | casa o NOME do executável, não a linha de comando; imune por construção |
| guardar o PID: `cmd & echo $!` e depois `kill -0 $PID` | não usa padrão nenhum; é o mais robusto quando se lançou o processo |
| esperar o **artefato**, não o processo (`until [ -f done.marker ]`) | o marcador é escrito pelo trabalho, então não há o que casar errado |

Preferir o PID quando o processo foi lançado por nós — as outras formas ainda casam processos homônimos de
outra sessão, que é uma segunda maneira de esperar a coisa errada.

## A classe maior

É o instrumento incluindo a si mesmo na medição. O mesmo formato aparece em
[VmRSS de backend PG inclui shared_buffers](vmrss-de-backend-pg-inclui-shared-buffers.md) (o contador soma
memória que não é do processo) e em [medir com carga concorrente](../failure-modes/contaminacao-por-concorrencia.md)
(o medidor compete com o medido). Antes de confiar num instrumento, pergunte se ele consegue se enxergar.

## Relacionados

- [invariant/vmrss-de-backend-pg-inclui-shared-buffers](vmrss-de-backend-pg-inclui-shared-buffers.md)
- [failure-mode/instrumento-cego-a-arquitetura](../failure-modes/instrumento-cego-a-arquitetura.md)
- [failure-mode/contaminacao-por-concorrencia](../failure-modes/contaminacao-por-concorrencia.md)
