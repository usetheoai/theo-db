---
type: Measurement
title: m177 — o caminho de embed satura em ~20 rps, e o gargalo do ADR 0007 deixa de ser hipótese
description: Mede sob concorrência o footgun que estava registrado desde junho e nunca medido; a saturação chega a 5,7× num ideal de 16×, com a p99 quadruplicando em troca de 5% de throughput.
resource: benchmarks/artifacts/m177/concurrency.json
tags: [benchmark, m177, embedding, concorrencia, gargalo, p99, adr-0007, sota, pg-net]
milestone: M177
generated: { by: claude-code/opus-5, at: 2026-08-07T22:30:00Z }
sources:
  - id: conc
    resource: benchmarks/artifacts/m177/concurrency.json
    title: Servidor de embeddings sob concorrência (1–16 clientes, 10 req cada)
  - id: pgnet
    resource: https://github.com/supabase/pg_net
    title: "pg_net — async HTTP from PostgreSQL via background worker + libcurl (Apache-2.0)"
---

Fecha uma lacuna que o [ADR 0007](/decisions/0007-synchronous-per-row-model-http.md) abriu em junho de
2026. Aquele ADR registrou que cada chamada segura **um backend PostgreSQL inteiro** pela latência do
modelo, e decidiu, com honestidade explícita, que *"máquina de fila é complexidade essencial apenas
depois de um gargalo medido"*. **O gargalo nunca havia sido medido.** Agora foi.

# ⚠️⚠️ SEGUNDA retratação (2026-08-08) — o ganho de 9,4× também não existe

A retratação abaixo trocou um erro por outro. Re-medido em **droplet `c-8` de CPU dedicada e ociosa**,
mesmo servidor, mesmos clientes:

| clientes | `OMP_NUM_THREADS=1` | sem limite | ganho |
|---|---|---|---|
| 1 | 97,5 rps · p50 10,1 ms | 97,9 rps · p50 9,8 ms | **1,00×** |
| 8 | 156,2 rps · p50 49,0 ms | 152,5 rps · p50 47,8 ms | **0,98×** |

**A configuração de thread não faz diferença nenhuma em hardware dedicado.** O 9,4× era, do começo ao
fim, artefato de contenção de CPU da máquina compartilhada.

**Mecanismo:** numa máquina disputada, `OMP_NUM_THREADS=1` dá ao ONNX uma única thread que compete com
dez containers pelo escalonador; sem o limite, ele abre várias e coletivamente arranca uma fatia maior de
CPU. Numa máquina ociosa não há disputa — e uma thread já basta, porque o modelo é pequeno e o gargalo
não é paralelismo intra-operador.

**Consequência prática:** a "maior alavanca medida do milestone" **não é alavanca**. A recomendação de
não estrangular o servidor continua sensata como higiene, mas **não vale 9,4×, e provavelmente não vale
nada** em máquina dedicada.

*(Os 97,5 rps a 1 cliente aqui não são comparáveis aos 193 rps do teste de stress no mesmo hardware: um
usa carga fixa por cliente, o outro carga sustentada por tempo, com textos de tamanhos diferentes.)*

# ⚠️ Primeira retratação (2026-08-07) — os números abaixo mediram a MINHA configuração, não o sistema

**O teto de ~20 rps era artefato meu.** O servidor foi iniciado com `OMP_NUM_THREADS=1` e
`ORT_NUM_THREADS=1` — herdados do experimento do hop, onde equalizar threads entre os dois braços era
*necessário* para a comparação ser justa. Carregar essa flag para um teste de **concorrência** estrangulou
o ONNX a um núcleo numa máquina de doze. Re-medido sem a restrição:

| clientes | com `OMP=1` (publicado antes) | **sem restrição (real)** | ganho |
|---|---|---|---|
| 1 | 3,5 rps · p50 298 ms | **32,9 rps · p50 30,3 ms** | **9,4×** |
| 4 | 11,1 rps | **55,6 rps** | 5,0× |
| 8 | 19,1 rps · p99 733 ms | **60,9 rps · p99 223 ms** | 3,2× |
| 16 | 20,1 rps · p99 1 887 ms | **61,1 rps · p99 1 358 ms** | 3,0× |

**A saturação real é ~61 rps, não ~20 rps.** Isso também resolve a divergência que a seção § Limites
honestos declarava como não investigada: a p50 de ~300 ms contra os 41–57 ms da
[fase 1](/benchmarks/m177-hop-vs-residencia-verdict.md) **era a flag**, e os 30,3 ms agora reconciliam.

**O que sobrevive:** a *forma* da curva. Ainda satura entre 8 e 16 clientes, e a p99 ainda degrada (223 →
1 358 ms) em troca de 0,3% de throughput. O teto mudou de valor, não de existência.

**A lição de método:** uma flag copiada de um experimento anterior, correta lá, invalidou o experimento
seguinte. As seções originais ficam abaixo sem edição, porque foram elas que fundamentaram a primeira
leitura.

# O número (medição original — estrangulada, ver retratação acima)

Servidor de embeddings local (ONNX, `bge-small-en-v1.5`), 10 requisições por cliente, warm-up descartado:

| clientes | throughput | escala real | p50 | p95 | **p99** | máx |
|---|---|---|---|---|---|---|
| 1 | 3,5 rps | 1,00× | 298 ms | 469 ms | 469 ms | 469 ms |
| 2 | 6,2 rps | 1,75× | 299 ms | 568 ms | 568 ms | 568 ms |
| 4 | 11,1 rps | 3,16× | 316 ms | 523 ms | 584 ms | 584 ms |
| 8 | 19,1 rps | **5,45×** | 377 ms | 700 ms | 733 ms | 733 ms |
| 16 | 20,1 rps | **5,73×** de um ideal de 16× | 615 ms | 1 192 ms | **1 887 ms** | 2 623 ms |

**A saturação está entre 8 e 16 clientes, em ~20 rps.** De 8 para 16 clientes o throughput sobe **5%**
enquanto a p99 **quadruplica** (469 → 1 887 ms). Passado esse ponto, adicionar carga só produz fila.

**Reportar média aqui esconderia o achado**: a média a 16 clientes é 615 ms, mas um em cada cem usuários
espera **1,9 segundo**. É a cauda que o usuário sente.

# O que isto valida, e o que não

**Valida** a preocupação do ADR 0007 — existe um teto, e ele é baixo em termos de banco de dados: ~20
consultas semânticas por segundo antes de a cauda degradar. Um sistema que sirva 100 buscas concorrentes
não cabe neste desenho sem mudança.

**Não valida** o mecanismo exato. Este experimento mediu o **servidor de embeddings** saturando, não
backends do PostgreSQL bloqueados — não havia PostgreSQL no laço. O ADR 0007 fala de um segundo custo,
somado a este: enquanto o servidor demora, o backend que chamou fica preso, e N chamadas concorrentes
prendem N backends, competindo com `max_connections`. **Esse segundo efeito continua não medido.**

# A segunda medição: conexão nova por chamada

`theodb_rs/src/http.rs` usa `minreq::post(endpoint)`, que **abre conexão nova a cada chamada** — sem
pool, sem keep-alive. Medido sobre loopback, sem modelo, só o canal:

| | round-trip (n=300) |
|---|---|
| conexão nova por chamada (o que o código faz hoje) | 1,016 ± 0,176 ms |
| conexão reutilizada (keep-alive) | **0,404 ± 0,160 ms** |

**Custo de abrir a conexão: ~0,6 ms sobre loopback** — irrelevante contra 300 ms de inferência.

**Mas contra um provedor remoto o mesmo defeito custa duas ordens de grandeza a mais.** O handshake
TCP+TLS medido até provedores reais, sem enviar nada: **32,0 ± 1,2 ms** (api.openai.com), 39,4 ms
(api.cohere.ai), 36,7 ms (api.voyageai.com). Sem reuso de conexão, **cada linha embedada paga esse
handshake outra vez**: uma tabela de 10 000 linhas gasta ~320 segundos apenas abrindo conexões.

## Um artefato de medição registrado, porque quase virou achado

A primeira coleta do keep-alive deu a conexão reutilizada **41 ms — quarenta vezes mais lenta** que a
conexão nova. Absurdo, e a assinatura é conhecida: ~40 ms é o temporizador de *delayed ACK* interagindo
com o algoritmo de Nagle numa conexão persistente. Com `TCP_NODELAY` nos dois lados, o número virou
0,404 ms. **O valor errado não foi publicado**; fica aqui porque um número plausível-mas-artefato é
exatamente o que um relatório apressado teria reportado como descoberta.

# O SOTA para este problema, e por que ele só resolve metade

O padrão do TheoDB — modelo fora do banco, chamado por endpoint configurável — **está alinhado ao SOTA**:
é o que o AlloyDB faz (`embedding()` como função SQL por linha, direcionador explícito do ADR 0007) e o
que o pgai faz.

Para o bloqueio do backend, a referência é o **[pg_net](https://github.com/supabase/pg_net)** (Supabase,
**Apache-2.0** — D1-limpo): HTTP assíncrono a partir do PostgreSQL, via **BackgroundWorker + libcurl**,
com a resposta entregue por **polling de tabela** (`_http_response`), não por callback.

**E é aqui que o SOTA se divide por caminho:**

| | Ingestão (`INSERT` → vectorizer) | Consulta (`SELECT` → busca) |
|---|---|---|
| pode ser assíncrona? | **sim** — o resultado é gravado depois | **não** — o vetor é necessário *nesta* query, para ordenar |
| modelo pg_net serve? | sim, e o TheoDB **já faz equivalente** com o worker do ADR 0016 | **não** — `request_id` não pode entrar num `ORDER BY … <=>` |
| o que resolve | fila (já existe) | **capacidade do servidor + reuso de conexão** |

O caminho de consulta **precisa** ser síncrono, e por isso a assincronia não é a resposta para ele. O que
resolve o caminho de consulta é o que este artefato mediu: teto de throughput do servidor e custo de
conexão — não uma máquina de fila.

# O flamegraph: 99% do tempo de requisição é o modelo, e não há gordura a cortar

Perfilado com `py-spy` (4 091 amostras a 99 Hz, 8 clientes concorrentes) — artefato:
`benchmarks/artifacts/m177/flamegraph-embed-server.svg`.

| frame | % do total amostrado | % **do caminho de requisição** |
|---|---|---|
| `do_POST` (todo o tratamento HTTP) | 19,02% | 100% |
| ├─ `onnx_embed` | 18,75% | **98,6%** |
| │   └─ `run` (InferenceSession) | 18,75% | 98,6% |
| └─ `tokenize` | 0,17% | 0,9% |
| HTTP + JSON + serialização (resto) | ~0,27% | **~1,4%** |

**Não existe overhead a otimizar no servidor.** Parsing HTTP, desserialização JSON, tokenização e
serialização da resposta somam ~1,4% do tempo. O gargalo **é o modelo**, no sentido literal: a chamada
`InferenceSession.run` é praticamente todo o custo.

Isso fecha a pergunta "dá para melhorar sem perder performance do modelo?" com uma resposta precisa: **no
código do servidor, não há o que melhorar** — não porque esteja otimizado, mas porque ele quase não faz
nada além de chamar o ONNX.

# A alavanca que existe, e ela é gratuita em qualidade

A configuração de thread deu **9,4× de throughput** — e a pergunta óbvia é se isso custou precisão
numérica, já que paralelismo altera a ordem de redução em ponto flutuante (IEEE-754 não é associativo — a
mesma classe de risco que o M169 registra para `sum(float8)`).

**Medido, não presumido.** Mesmos três textos, embedados sob as duas configurações:

| | resultado |
|---|---|
| vetores byte-idênticos | **3 de 3** |
| maior diferença absoluta em qualquer dimensão | **0,000e+00** |
| similaridade de cosseno mínima entre os pares | **1,0000000000** |

**Zero divergência.** O ganho de 9,4× não custa um bit de qualidade — é o mesmo modelo, os mesmos pesos,
a mesma saída. Instrumento: `benchmarks/m177_thread_equivalence.py`.

As demais alavancas **não** são gratuitas e ficam fora deste artefato: quantização int8 do modelo (troca
qualidade por velocidade — o acervo já mede que [quantização compra memória, não QPS](/features/19-quantizacao-vetorial.md)),
modelo menor (o `MiniLM-multilingual` faz 16,0 ms contra 59,8 ms do `e5-large`, mas com qualidade
diferente e ainda não medida), e replicar o processo (custo de operação, não de qualidade).

# Correções que estes números indicam

1. **Não estrangular o servidor de embeddings.** A maior alavanca medida — **9,4×** — e a mais barata:
   não passar `OMP_NUM_THREADS=1`. Byte-idêntico na saída. Pertence à documentação de operação, e o
   `sql-embeddings.md` hoje não diz nada sobre threads.
2. **Reuso de conexão no cliente HTTP.** Barato em loopback (~0,6 ms/chamada), **decisivo contra
   provedor remoto** (~32 ms/chamada). Não muda semântica.
3. **Capacidade é ~61 rps por instância** nesta máquina, com a cauda degradando acima de 8 clientes.
   Escalar é replicar o processo — operação, não arquitetura.
4. **A fila assíncrona não é prioridade para a consulta** — só para a ingestão, onde já existe.
5. **Otimizar o servidor não paga.** O flamegraph mostra ~1,4% fora do modelo; qualquer reescrita do
   transporte disputa esse 1,4%.

# Limites honestos desta medição

- **Uma única instância**, num único modelo (384d), numa máquina **não dedicada** (12 cores, 15 GB, dez
  containers ativos). O teto de ~20 rps é desta configuração, não um número universal.
- **Sem PostgreSQL no laço.** O efeito de backends bloqueados contra `max_connections` — o coração do
  ADR 0007 — segue não medido. Este artefato mede o servidor, não o banco.
- A latência p50 de ~300 ms a 1 cliente é **maior** que os 41–57 ms medidos em
  [m177 fase 1](/benchmarks/m177-hop-vs-residencia-verdict.md) para o mesmo modelo. As condições diferem
  (texto mais longo, máquina mais carregada, `ThreadingHTTPServer` sob threads). **A divergência não foi
  investigada**, e por isso o veredito se apoia na **forma da curva** — saturação e explosão de cauda —
  não nos valores absolutos.

# Relacionados

- O ADR que registrou o footgun e pediu medição antes da fila: [ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)
- A fila que já existe para a ingestão: [ADR 0016](/decisions/0016-m54-vectorizer-worker-mechanism.md)
- Custo e residência do modelo: [m177 fase 1](/benchmarks/m177-hop-vs-residencia-verdict.md)
- O prior art de embedding local: [prior art](/references/embedding-local-como-extensao-2026-08.md)
- O desenho atual: [embeddings em SQL](/guides/sql-embeddings.md)
