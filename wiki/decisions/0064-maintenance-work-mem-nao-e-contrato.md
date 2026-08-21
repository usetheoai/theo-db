---
type: Decision
title: ADR 0064 — `maintenance_work_mem` não é um contrato de memória neste produto, e o que passa a ser
description: Dois componentes tratam o mesmo knob de formas incompatíveis — o colunar consome ~7× ele, o build do HNSW o ignora. Não são dois bugs isolados: é a ausência de um contrato. A regra que passa a valer é projetar e recusar nomeando o número, nunca tentar e morrer.
tags: [adr, memoria, maintenance-work-mem, hnsw, colunar, oom, contrato, b-076]
adr_id: "0064"
adr_status: Accepted
decision_date: 2026-08-20
generated: { by: claude-code/opus-5, at: 2026-08-20T00:00:00Z }
---

Decisão irmã sobre o mesmo eixo — o que a esteira mede e sob qual autoridade:
[ADR-0062](0062-portao-antes-do-merge.md).

# Contexto

Duas medições independentes, em componentes diferentes, sobre o **mesmo** parâmetro:

| componente | o que faz com `maintenance_work_mem` | evidência |
|---|---|---|
| build do `theodb_hnsw` | **ignora** | 250k×128 com `mwm=64MB` → **606 MB** de pico; 1M → **1871 MB**; 20M → OOM-kill num host de 16 GB (`anon-rss:10033724kB`) |
| `flush_pending` do colunar | consome **~7×** | `mwm=2GB` num `INSERT ... SELECT` de ~100M×105 → **23,4 GB** de RSS anônimo e OOM-kill (issue #221) |

Um ignora o orçamento; o outro o excede por quase uma ordem de grandeza. **O knob promete um
orçamento e nenhum dos dois o cumpre.**

## Por que isso não são dois bugs

Corrigir cada um isoladamente produziria dois números arbitrários e nenhuma garantia. O que falta é
anterior aos dois: **este produto nunca declarou o que `maintenance_work_mem` significa nele.**

E o custo cai no lugar mais caro possível. O `maintenance_work_mem` é o knob que toda documentação
de PostgreSQL manda subir para carga em massa e construção de índice. Um operador que faz a coisa
documentada — subir o knob — recebe um **servidor reiniciado**. A ação correta é punida, e a punição
não tem mensagem.

# Decisão

**`maintenance_work_mem` não é honrado como teto de memória por estes componentes, e o produto passa
a dizer isso em vez de deixar o operador descobrir pelo OOM killer.**

Em lugar dele, a regra que passa a valer para todo caminho de build ou carga em massa:

> **Projete antes de tentar, e recuse nomeando o número.** Um componente que não consegue respeitar
> um teto deve estimar seu pico, compará-lo com um orçamento declarado, e falhar cedo com uma
> mensagem que nomeie a projeção, o orçamento e o knob que o operador pode mexer — nunca tentar e
> morrer.

Três propriedades, e cada uma responde a um jeito de errar:

- **Orçamento explícito e por componente.** `theodb_hnsw.build_memory_mb` (B-076) é o primeiro. Um
  knob por componente é mais honesto que um global que ninguém cumpre: ele diz de quem é o custo.
- **Default derivado do host, não zero.** Sem valor declarado, o orçamento vem do `MemAvailable` —
  que é o que o OOM killer de fato olha. Um default `0` = sem verificação preservaria o defeito.
- **Fail-open quando não dá para saber.** `/proc/meminfo` ausente ou ilegível → sem verificação.
  Recusar um build por incapacidade **nossa** de ler a memória transformaria uma incerteza em erro
  do operador.

# O que isto NÃO resolve, dito em vez de omitido

**Não faz o build do HNSW caber em menos memória.** HNSW não é streamável como o IVF: a construção
do grafo precisa de acesso aleatório aos vizinhos, então a rota de memória limitada do M96
(`build_stream::should_stream`) não se aplica por mais que o gate dela mencione opções do IVFFlat.
Trocar a recusa por uma construção incremental é outro trabalho, muito maior, e este ADR não o
promete.

**Não corrige a issue #221.** O colunar continua consumindo ~7× o knob. O que este ADR faz é dizer
que a correção dele deve seguir a mesma regra — projetar e recusar — em vez de escolher um fator de
segurança novo. Sem isso, o próximo componente inventa o terceiro comportamento.

**O modelo do HNSW tem dois pontos.** `184 MB + 3,45 × corpus` reproduz 250k e 1M exatamente, na
mesma dimensão e com o mesmo `m`. O termo linear no corpus faz ele escalar com a dimensão, o que é a
física do problema — o corpus é `n × dim × 4` —, mas isso é **suposição do modelo, não medição**. Um
terceiro ponto noutra dimensão confirma ou derruba, e ele espera o host que o [[B-075]] espera.

# Consequências

- Um operador que sobe o `maintenance_work_mem` esperando acelerar a carga passa a receber uma
  recusa com números, em vez de um servidor reiniciado.
- Um componente novo que faça build ou carga em massa herda a regra: projetar e recusar. É a única
  coisa que impede o terceiro comportamento incompatível.
- A comparação com o [[B-058]] fica possível: sem contrato de memória, qualquer número de carga
  colunar que publiquemos depende de um estado de tuning que ninguém declarou.
