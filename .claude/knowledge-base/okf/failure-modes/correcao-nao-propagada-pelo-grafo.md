---
type: Failure Mode
title: Corrigir o conceito-fonte e deixar os que o citam com o valor antigo
description: Um número corrigido em um arquivo sobrevive nos siblings que o citam — inclusive no índice — e o registro afirma que a correção foi concluída.
tags: [conhecimento, propagacao, okf, honestidade]
timestamp: 2026-07-30T00:00:00Z
---

# Corrigir o conceito-fonte e deixar os que o citam com o valor antigo

## A assinatura

A correção é real, está certa, e está **num arquivo só**. Os outros que citam o mesmo valor continuam publicando
o número refutado — e o registro da correção diz que ela foi concluída.

## O caso — este bundle, três vezes na mesma correção

Uma única correção (`mwm × 7` → `× 8`) foi aplicada ao conceito-fonte. O round 3 do review achou o valor antigo
**vivo em três outros lugares**:

| Onde | O que ainda dizia |
|---|---|
| `invariants/chunk-group-e-a-unidade-de-tudo` | `multiplicador ×7` — e essa tabela é o mapa que o conceito manda usar ao ler qualquer caminho colunar |
| `techniques/medir-antes-de-filar` | `~510 MB` para `mwm=128MB` (o valor pertence a 64MB; ×8 dá ~1,0 GiB) |
| `measurements/index.md` | `~31%` — o índice, que é a **porta de entrada** do gatilho "isso já foi medido?" |

Agravante: o `log.md` afirmava **"3 defeitos NOVOS. Todos tratados."** enquanto os três acima estavam no disco.
Um registro que declara tratamento inexistente é pior que nenhum registro.

## Por que a classe é específica deste formato

O OKF é um **grafo**: o valor de um conceito aparece na prosa de vizinhos, nos `Relacionados`, e — sempre — na
descrição que o `index.md` da categoria replica. Corrigir "o conceito" é corrigir **um nó**; o grafo não se
atualiza sozinho.

E os gates não pegam: `C2` valida que os links **resolvem**, `C3` que os índices listam **os arquivos certos** —
nenhum compara **conteúdo**. Por desenho: um checker que julgasse prosa seria
[cobertura-alegada-sem-execucao](cobertura-alegada-sem-execucao.md).

## A regra operacional (uma linha, e é mecanizável por quem corrige)

> **Antes de dar uma correção numérica por fechada, `grep` o valor ANTIGO no bundle inteiro.**

```bash
grep -rn '×7\|~510 MB\|31%' .claude/knowledge-base/okf/
```

Se voltar qualquer coisa que não seja a própria nota de correção, a correção não acabou. E ao regenerar índices
de categoria, fazê-lo **depois** de editar os frontmatters — não antes, senão o índice congela o valor velho.

## Relacionados

- [failure-mode/crenca-intermediaria-congelada](crenca-intermediaria-congelada.md) — a origem em transcript; esta é a propagação no grafo
- [technique/nenhuma-alegacao-sem-medicao](../techniques/nenhuma-alegacao-sem-medicao.md)
- [failure-mode/cobertura-alegada-sem-execucao](cobertura-alegada-sem-execucao.md)
