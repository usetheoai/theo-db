---
type: Measurement
title: b102 — os números colunares publicados descrevem uma configuração que o default do produto não entrega
description: "O arnês liga theodb.enable_columnar_agg (opt-in, default OFF) para medir o colunar com pushdown — corretamente — e verificava a GUC sem registrá-la em lugar nenhum. A diferença medida (911 ms contra 74 ms, 12×) saiu de psql à mão e NÃO é publicável sob o B-069; o achado, porém, não depende dela — o artefato não declarar a configuração se verifica abrindo o artefato. O system.json de uma corrida publicada traz 14 GUCs de servidor e nenhuma de sessão; dos 53 conceitos que publicam número colunar, 3 mencionavam a GUC."
tags: [colunar, arnes, medicao, honest-negative, integridade, b-102]
item: B-102
procedencia: fora-do-arnes
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

# Procedência dos números desta página — e o que ela custa a eles

*Acrescentado no mesmo dia, algumas horas depois, ao revisar o [[B-069]] contra o próprio trabalho.*

**Os 911 ms e os 74 ms não saíram do arnês.** Foram obtidos com `psql` à mão, num servidor que eu
tinha de pé por outra razão, durante a investigação do [[B-099]]. Não há bundle, não há
`validation.json`, não há registro de ambiente, e **ninguém consegue reproduzi-los a partir de um
artefato** — que é precisamente o requisito que o [[B-069]] impõe a número publicável, e que o
`docs/methodology/PUBLICATION.md` do arnês declara desde 2026-08-20.

Escrevi esta página horas depois de listar o B-069 entre os itens abertos, e ainda assim publiquei
um número por fora. Registro isto aqui em vez de reescrever a página em silêncio, porque uma
retratação apagada é a forma mais cara de erro que este projeto conhece.

**O que a procedência fraca derruba, e o que ela não derruba:**

| Afirmação | Como foi obtida | Vale? |
|---|---|---|
| O `system.json` traz 14 GUCs de servidor e nenhuma de sessão | leitura direta do artefato publicado | **sim** — o artefato está no disco e qualquer um o abre |
| O `manifest.json` não cita a GUC | idem | **sim** |
| 3 de 53 conceitos colunares mencionam a GUC | `grep` sobre `wiki/benchmarks/` | **sim** |
| O adapter verificava e descartava a verificação | leitura do código, agora coberta por 4 testes | **sim** |
| **A diferença é de 911 ms para 74 ms (12×)** | **`psql` à mão, sem bundle** | **não é publicável** |

**O achado desta página não depende do número.** Ele é: *o artefato não declara a configuração*, e isso
se verifica abrindo o artefato. A **magnitude** do que a configuração decide é que está mal-medida — e
ela importa, porque é o que diz ao leitor se a omissão é grave ou trivial.

**Fica em aberto no [[B-069]]:** repetir a comparação `enable_columnar_agg` off × on por
`theodb-bench run`, com bundle, e substituir a linha da tabela acima **por acréscimo**, mantendo a
versão de hoje visível. Até lá, a ordem de grandeza é indicativa e está dita como tal — não é
"aproximadamente 12×", é *"medido por fora, e por isso não publicável"*.

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
