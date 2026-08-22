---
type: Measurement
title: b102 — os números colunares publicados descrevem uma configuração que o default do produto não entrega
description: "O arnês liga theodb.enable_columnar_agg (opt-in, default OFF) para medir o colunar com pushdown — corretamente — e verificava a GUC sem registrá-la em lugar nenhum. Medido: count(*) a 2M custa 911 ms no default contra 74 ms com ela ligada, 12×. O system.json de uma corrida publicada traz 14 GUCs de servidor e nenhuma de sessão; dos 53 conceitos que publicam número colunar, 3 mencionavam a GUC."
tags: [colunar, arnes, medicao, honest-negative, integridade, b-102]
item: B-102
generated: { by: claude-code/opus-5, at: 2026-08-22T16:00:00Z }
---

Peças: [o instrumento reporta o pedido](../guides/instrumento-reporta-o-pedido.md), do qual isto é a
quinta ocorrência e a imagem espelhada; [b058 — o crossover do colunar](b058-crossover-colunar.md);
[b061 — o crossover colunar](b061-columnar-crossover.md).

# O que foi medido

Mesma tabela, mesmo servidor, mesma sessão, variando **um** botão:

| Configuração | `count(*)` a 2M linhas | Plano |
|---|---|---|
| `theodb.enable_columnar_agg = off` — **o default do produto** | **911 ms** | Seq Scan |
| `theodb.enable_columnar_agg = on` — o que o arnês mede | **74 ms** | Custom Scan (`theodb_columnar_agg`) |

**12×.** Consistente com a medição anterior a 1M (1407 ms → 108 ms, 13×), registrada em comentário de
teste desde a construção do portão de residência.

# O que estava errado, e o que não estava

**Ligar a GUC não era o erro.** É deliberado e está justificado no código do arnês: medir o colunar sem
o pushdown é um caminho que já se sabe perder para o heap, e publicá-lo como "nosso colunar" seria o
mesmo defeito que medir o ScaNN com o quantizador AH desligado. O adapter inclusive **verificava** que
a GUC pegou, usando a mesma função que verifica os botões de busca.

**O erro era descartar a verificação.** O caminho vetorial atribuía o resultado a
`_effective_search_parameters` e o levava até `points[].parameters`. O caminho analítico chamava a
mesma função e ignorava o retorno. Consequência medida nos artefatos:

- `system.json` de uma corrida publicada: **14** GUCs, todas de servidor (`shared_buffers`, `work_mem`,
  `maintenance_work_mem`, `fsync`, `wal_level`, …). **Zero** de sessão.
- `manifest.json`: não cita a GUC.
- **3 de 53** conceitos que publicam número colunar em `wiki/benchmarks/` a mencionam — e num deles a
  menção foi acrescentada em 2026-08-22, ao corrigir outro item.

# A consequência, que não é hipotética

**O default do TheoDB não é a configuração em que os números colunares publicados foram obtidos.** Quem
instala o produto e roda uma agregação sobre uma tabela colunar recebe o caminho de 911 ms até ligar a
GUC por conta própria.

E o custo já tinha aparecido antes de alguém procurá-lo: o [[B-101]] foi registrado sobre uma premissa
errada e morreu, porque duas medições rodaram em configurações diferentes e **nenhum artefato dizia
qual era qual**. A ausência de declaração não é neutra — ela produziu uma hipótese falsa e o trabalho
de matá-la.

# O conserto

`effective_analytical_settings()` — irmã de `effective_search_parameters()` — devolve as GUCs de sessão
que o adapter aplicou **e teve confirmadas**, e `bench/analytical.py` as leva até `points[].parameters`
pelo mesmo caminho que o vetorial já usava. Quatro testes cobrem os dois lados: com GUC e sem GUC, no
acessor e no ponto. Suíte do arnês: 1106 passed, 3 skipped.

**O que o conserto NÃO faz:** ele não reconstrói os artefatos já publicados. Os bundles anteriores
continuam sem a declaração, e a única correção possível para eles é esta página — que é o motivo de ela
existir.

# A regra que sai daqui

> **Declarar e verificar não basta. O que foi verificado tem de sair no artefato.**
> Um bundle onde a GUC foi ligada e confirmada é byte a byte indistinguível, para quem o lê, de um
> bundle onde ela nunca foi tocada.
