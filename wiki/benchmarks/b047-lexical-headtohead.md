---
type: Measurement
title: b047 — TheoDB × Elasticsearch × OpenSearch no MS MARCO, mesma máquina
description: Com pré-processamento casado, a paridade de ranqueamento é DEMONSTRADA por teste pareado (p=0,48, IC estreito em torno de zero, n=6.980) — e o throughput não é: o Elasticsearch faz 4,3× o nosso QPS.
tags: [benchmark, lexical, bm25, elasticsearch, opensearch, honest-negative, b047]
item: B-047
generated: { by: claude-code/opus-5, at: 2026-08-13T12:15:00Z }
sources:
  - id: run
    resource: benchmarks/vectordbbench/results-lexical/
    title: logs e JSON brutos das cinco corridas
    last_modified: 2026-08-13
---

# O resultado, sem rodeio

Com o pré-processamento casado — todos os três motores stemizando e removendo stopwords em inglês — a
**qualidade de ranqueamento é paridade** e o **throughput não é**.

| Motor | analisador | NDCG@10 | recall@10 | MRR | QPS | p99 serial | carga |
|---|---|---|---|---|---|---|---|
| **TheoDB** `b044` | `theodb_en` | **0,7351** | **0,8464** | **0,7034** | 1.794,0 | 4,3 ms | **28,9 s** |
| Elasticsearch 9.1.2 | `english` | 0,7343 | 0,8449 | 0,7029 | **7.638,5** | **1,9 ms** | 59,3 s |
| OpenSearch 2.17.1 | `english` | 0,7344 | 0,8450 | 0,7030 | 7.026,7 | 2,2 ms | 59,1 s |

**Qualidade: paridade, e agora demonstrada.** O [[B-045]] aplicou teste de permutação pareada sobre as
**6.980 consultas** do caso:

| comparação | diff médio (NDCG) | IC 95% | p (permutação) | p (t) | vitórias/derrotas/empates | `d_z` |
|---|---|---|---|---|---|---|
| TheoDB vs Elasticsearch | +0,00066 | [−0,0011, +0,0025] | **0,477** | 0,475 | 233 / 263 / 6.484 | 0,009 |
| TheoDB vs OpenSearch | +0,00068 | [−0,0011, +0,0025] | **0,466** | 0,463 | 235 / 268 / 6.477 | 0,009 |
| Elasticsearch vs OpenSearch | +0,00002 | [−0,0002, +0,0002] | 0,912 | 0,843 | 9 / 10 / 6.961 | 0,002 |

**É a espécie certa de não-significância.** Um `p` alto pode significar duas coisas opostas: equivalência, ou
falta de poder para detectar uma diferença real. Aqui o IC é **estreito e centrado em zero** — largura de
0,0036 em NDCG, com n=6.980 —, o que é evidência de **equivalência**, não ausência de evidência. O `d_z` de
0,009 confirma: o tamanho de efeito é praticamente nulo.

Em 6.980 consultas, **6.484 empatam exatamente** entre TheoDB e Elasticsearch, e as 496 restantes se dividem
quase igualmente (233 a 263). Não é um empate na média que esconde variação — é empate consulta a consulta.

Os arrays por consulta dos três sistemas estão em `benchmarks/significance/per-query/b047.json`, para que um
terceiro recomponha o teste — inclusive com outro método, se discordar da escolha.

**Throughput: o Elasticsearch faz 4,3× o nosso QPS**, com p99 2,3× menor. O OpenSearch faz 3,9×.

**Carga: somos 2× mais rápidos** — 28,9 s contra ~59 s para os mesmos 100.000 documentos.

# A comparação que eu quase publiquei, e por que estaria errada

A primeira rodada usou o **mapeamento que o próprio arnês configura** para Elastic e OpenSearch:
`{"text": {"type": "text"}}`, sem analisador nomeado — o que resolve para o `standard`, que **não stemiza**
(verificado direto: `jumping` e `jumps` saem como tokens distintos, `the` é mantido).

| Motor | analisador | NDCG@10 | recall@10 | QPS |
|---|---|---|---|---|
| TheoDB `b044` | `theodb_en` (stemming) | 0,7351 | 0,8464 | 1.794,0 |
| Elasticsearch 9.1.2 | `standard` (sem stemming) | 0,6908 | 0,7972 | 6.634,0 |
| OpenSearch 2.17.1 | `standard` (sem stemming) | 0,6909 | 0,7973 | 6.101,3 |

Lida sozinha, essa tabela diz **+6,4% de NDCG para o TheoDB**. É verdade e é enganosa: a vantagem inteira é a
assimetria de pré-processamento, não o ranqueamento. Ao dar `english` aos dois concorrentes — stemming e
stopwords equivalentes aos nossos — o NDCG deles sobe de 0,6908 para 0,7343 e **a vantagem desaparece**.

É o mesmo erro que o [b035](b035-theodb-vs-pgvector-pg18.md) documentou no pilar vetorial, na terceira
variação: lá o **parâmetro** era igual e o ponto de operação não; no [b044](b040-theodb-fts-msmarco.md) o
**rótulo** era igual e a máquina não; aqui a **configuração padrão** era a de cada um, e o pré-processamento
não.

**As duas rodadas são publicadas porque as duas são verdadeiras**, e respondem perguntas diferentes:

- *product-default* responde "o que um usuário recebe ao instalar cada um e seguir o arnês" — e ali o TheoDB
  entrega melhor qualidade, porque o nosso padrão stemiza e o mapeamento do arnês para os outros não.
- *analisador casado* responde "qual motor ranqueia melhor" — e ali é empate.

A segunda é a que importa para julgar o motor. A primeira é a que importa para julgar o padrão.

# O que foi medido

| | |
|---|---|
| Caso | `FTSBm25Performance` — MS MARCO Small, **100.000 documentos**, 6.980 consultas com qrels |
| k | 10 |
| Máquina | droplet DigitalOcean `g-16vcpu-64gb`, nyc3, **IP 165.227.108.7** (efêmero, destruído ao fim) |
| CPU | Intel Xeon Platinum 8358 @ 2,60 GHz, 16 vCPU, 62 GB |
| Motores | TheoDB `b044` (PG 18.4) · Elasticsearch **9.1.2** · OpenSearch **2.17.1** |
| Memória | 4 GB de heap para cada JVM; `shared_buffers=4GB` no TheoDB |
| Corridas | 5 no total — 3 em product-default, 2 com `english` |

Cada corrida verificou o que estava de fato ativo, em vez de confiar no rótulo: o TheoDB imprime
`stemming ativo: 1` medido por `bm25_build` + consulta flexionada, e o analisador dos concorrentes foi lido
de volta do `_settings` de cada índice após a corrida.

# O que esta comparação NÃO cobre

- **O empate de NDCG agora É testado** (acima); a **diferença de QPS não**. QPS não tem valor por consulta,
  então o pareado não se aplica a ele — o caminho seria N corridas repetidas, e é item separado. Os 4,3× são
  grandes demais para ser ruído desta magnitude, mas isso é leitura, não teste.
- **Memória não é equivalente, só não-grosseira.** 4 GB de heap JVM e 4 GB de `shared_buffers` não são a
  mesma coisa — um Postgres também usa o page cache do sistema. O que se evitou foi a assimetria de dar
  16 GB a um e 1 GB a outro.
- **Um corpus, um tamanho, um idioma.** MS MARCO 100K em inglês.
- **Nenhum ajuste de `k1`/`b`.** O TheoDB não os expõe; os outros dois expõem e não foram tocados. Todos
  rodaram com o BM25 padrão de cada um.
- **Sem operadores de consulta** de nenhum lado — o caso usa consultas em linguagem natural.
- **Uma corrida do OpenSearch com `english` foi descartada.** O template não pegou (padrão
  `vdb_bench_indice*` contra o índice real `vdb_bench_index`), e os números saíram idênticos aos do
  product-default. Está nos brutos e **não** entra em nenhuma tabela; a corrida válida é a que teve o
  analisador confirmado em `_settings`.

# Dois defeitos de ferramenta encontrados no caminho

**O cliente OpenSearch do arnês era inrodável.** `REPLICA_HEALTH_TIMEOUT = "30m"` — sintaxe de duração do
Elasticsearch — era passado como `timeout=` numérico ao transporte, que levanta
`ValueError` antes de qualquer requisição sair. `_update_replicas` chama esse caminho
incondicionalmente do `optimize()`, então **toda** corrida de OpenSearch falhava. Corrigido no fork
(`"30m"` → `1800`), fora do escopo do nosso cliente e declarado como tal — candidato a PR upstream.

**O `elasticsearch-py` do extra é não-pinado.** Instala 9.x, e contra um servidor 8.15 o cabeçalho de
compatibilidade produz um `BadRequestError(400, 'None')` opaco na criação do índice. Alinhar o servidor em
9.1.2 resolveu. Não é bug do arnês nem do Elastic — é uma armadilha de versão que custa uma corrida inteira
para diagnosticar, e que o artefato registra para quem vier depois.

# Reproduzir

```bash
# os três motores na mesma máquina
docker compose -f benchmarks/vectordbbench/lex-compose.yml up -d
uv venv --python 3.11 /tmp/vdbb && . /tmp/vdbb/bin/activate
uv pip install "vectordb-bench[theodb,elastic,opensearch] @ git+https://github.com/usetheoai/VectorDBBench@theodb"
./benchmarks/vectordbbench/run-lexical.sh          # product-default
./benchmarks/vectordbbench/run-lexical-english.sh  # analisador casado
```

Logs e JSON brutos em `benchmarks/vectordbbench/results-lexical/`; a spec da máquina em `machine.txt`.

# Política

Esta corrida é o [ADR-0061](../decisions/0061-benchmark-oficial-por-pilar.md) executado por inteiro pela
primeira vez: arnês de terceiros, concorrentes na mesma corrida e na mesma máquina, qualidade ao lado de
velocidade, e ponto de operação casado. Ela também **estende** a regra — o que precisa ser casado não é só o
parâmetro e a máquina, é o **pré-processamento**.
