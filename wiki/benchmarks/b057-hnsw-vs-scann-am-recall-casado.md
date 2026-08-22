---
type: Measurement
title: b057 — contra o ScaNN do AlloyDB casado por recall, ganhamos abaixo de 0,96 e perdemos acima de 0,99
description: "Primeira comparação do nosso HNSW contra o access method scann do AlloyDB Omni na mesma máquina, casada por recall, em SIFT real. Cinco pares casaram: vencemos os três até recall 0,961 (dz de -0,23 a -2,02) e perdemos os dois em 0,994+ (dz +0,98 e +0,22). O resultado se divide por regime, e o pareamento posicional anterior escondia isso inteiro."
tags: [vetorial, hnsw, scann, alloydb, headtohead, recall-casado, b-057, b-103]
item: B-057
procedencia: arnes
generated: { by: claude-code/opus-5, at: 2026-08-22T19:40:00Z }
---

Artefato: `benchmarks/artifacts/b057/recall-casado/recall-casado.txt`.
Peças: [b058 — TPC-H contra o Omni](b058-tpch-headtohead-omni.md);
[o instrumento reporta o pedido](../guides/instrumento-reporta-o-pedido.md).

# O que foi medido

SIFT real (`sift-128-euclidean`, 100 mil × 128d, 500 consultas, `k=10`), um droplet
`g-16vcpu-64gb`, os dois servidores no mesmo host. **Intercalado consulta a consulta**, com a ordem
alternando — a consulta *i* vai aos dois antes de passar para a seguinte, para que qualquer coisa que
se mova na escala de minutos mova os dois lados juntos.

O produto das duas varreduras é percorrido e **só os pares dentro de 0,01 de recall recebem veredito**.
Dos 40 pares, **cinco casaram**.

| nosso ponto | recall | ponto do Omni | recall | vencedor | `dz` |
|---|---|---|---|---|---|
| `ef_search=16` | 0,8670 | `leaves_to_search=20, rerank=25` | 0,8756 | **TheoDB** | −0,23 |
| `ef_search=64` | 0,9610 | `leaves_to_search=20, rerank=100` | 0,9556 | **TheoDB** | −0,56 |
| `ef_search=64` | 0,9610 | `leaves_to_search=20, rerank=400` | 0,9576 | **TheoDB** | **−2,02** |
| `ef_search=256` | 0,9944 | `leaves_to_search=80, rerank=100` | 0,9956 | **AlloyDB Omni** | +0,98 |
| `ef_search=256` | 0,9944 | `leaves_to_search=80, rerank=400` | 1,0000 | **AlloyDB Omni** | +0,22 |

Todos com `p = 0,0001`, `n = 500`, teste pareado. Faixa de recall coberta: **nós de 0,8670 a 0,9944**;
**Omni de 0,6898 a 1,0000**.

# A leitura, e ela se divide

**Vencemos no regime de recall baixo e médio; perdemos no regime alto.** O ponto de virada está entre
0,961 e 0,994. Não é um veredito único, e insistir num seria escolher a metade conveniente.

**A força de cada resultado é diferente, e o `dz` diz qual.** A vitória mais forte é `dz = −2,02`
(493 das 500 consultas), a 0,961 de recall. A vitória mais fraca é `dz = −0,23` (255 de 500) — mal
distinguível de empate apesar do `p` pequeno, porque `n = 500` torna significativa uma diferença
minúscula. A derrota mais forte é `dz = +0,98` (424 de 500) a 0,994.

**As ressalvas de recall apontam em direções opostas, e isso importa:**

- Na vitória mais fraca **nós** estávamos 0,0086 abaixo em recall — parte da vantagem é trabalho que
  não fizemos. É a vitória a descontar.
- Nas duas vitórias sólidas o **Omni** estava abaixo (0,0054 e 0,0034) — nossa vantagem ali está, se
  algo, **subestimada**.
- Nas duas derrotas **nós** estávamos abaixo (0,0012 e 0,0056) — estávamos fazendo menos trabalho **e
  ainda assim mais lentos**. As derrotas também estão subestimadas.

# O que isto muda no veredito LOCKED do North Star

O [[ADR-0035]] declarou a superioridade de QPS sobre o ScaNN **não-alcançável**, com gap de ~25–44×.
Aquele veredito mediu a **biblioteca ScaNN**, e o [[B-057]] existe precisamente porque o concorrente
real é um **access method do PostgreSQL**, que paga o mesmo imposto de MVCC, WAL e página que nós.

**Medido agora contra o access method: em latência por consulta, a 100 mil vetores, não há gap de
ordem de grandeza em nenhum dos dois sentidos.** Os `mean diff` vão de −0,774 ms a +0,658 ms.

**O que isto NÃO autoriza dizer.** Não é QPS multi-cliente, não é 1 milhão de vetores, e não toca no
mecanismo que o ADR-0035 credita pelo gap. O ADR mediu outra coisa e continua descrevendo o que mediu;
esta página mede o que ele não mediu, e as duas convivem. **Nada aqui reabre o veredito LOCKED** — isso
é decisão do dono, e o que esta medição entrega é o dado que faltava para ela.

# Ressalvas

- **Perfil `research`** — não publicável pelo padrão do próprio projeto. Ver a nota de perfis no
  `README.md`: o único perfil publicável exige preflight de `cpu_governor`, que máquina virtualizada
  não expõe.
- **Uma repetição por ponto.** O teste pareado tem `n = 500` **consultas**, não 500 corridas — ele
  mede a diferença consulta a consulta dentro de uma execução, e é forte para isso. Não substitui
  repetição entre execuções.
- **100 mil vetores.** O `sift1m` existe registrado e não foi rodado aqui.
- **Latência de cliente único.** Vazão sob concorrência é outro eixo, medido em [[m72-qps-multiclient]].

# Como este número quase não existiu

Duas corridas antes desta devolveram **`no verdict` em tudo**, e a conclusão fácil era que o ScaNN do
Omni satura em 0,72 de recall. **Era falsa.** O `cmd_head2head` pareava os pontos por **posição**
(`zip`), e a varredura do ScaNN é cartesiana — os três primeiros pontos do produto têm todos
`num_leaves_to_search=5`, o canto mais raso. Os pontos que casariam nunca eram medidos.

O casamento por recall **já existia** em `compare.py`, e não era chamado ([[B-103]]). Consertado, os
mesmos sistemas na mesma máquina produziram cinco vereditos — três a nosso favor e dois contra.

> **A leitura falsa era a favorável a nós.** É o argumento mais forte que este projeto tem para manter
> o portão de recall casado: sem ele, a corrida teria publicado uma vantagem que não existe, e o
> instrumento não teria reclamado.
