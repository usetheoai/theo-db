---
slug: b035-vectordbbench-client
items: [B-035]
date: 2026-08-12
branch: workspace
---

# Um cliente TheoDB no VectorDBBench que recusa medir o que não pode honrar

## Goal

Entregar um cliente `theodb` no VectorDBBench, em fork de diff mínimo, que (a) executa uma corrida real com
recall medido contra o TheoDB, (b) **falha alto** quando o caso pede um parâmetro de build que o TheoDB não
honra, em vez de ignorá-lo em silêncio, e (c) produz um artefato comparável contra o pgvector **na mesma
versão de PostgreSQL**, publicado com o que a corrida não cobre dito explicitamente.

## Baseline Context

### Files that will be touched

Dois repositórios. O cliente vive no fork; o arnês de reprodução vive aqui.

**Fork `usetheoai/VectorDBBench`, branch `theodb`, a partir de `zilliztech/VectorDBBench@5d0d314`:**

| Arquivo | Estado | LoC estimadas | Papel |
|---|---|---|---|
| `vectordb_bench/backend/clients/theodb/__init__.py` | novo | ~10 | re-exports |
| `vectordb_bench/backend/clients/theodb/config.py` | novo | ~150 | `TheoDBConfig`, `TheoDBIndexConfig` (abstrata), `TheoDBHNSWConfig`, registro de caso |
| `vectordb_bench/backend/clients/theodb/theodb.py` | novo | ~200 | `TheoDB(VectorDB)` — o adaptador |
| `vectordb_bench/backend/clients/theodb/cli.py` | novo | ~90 | comando `theodb-hnsw` |
| `vectordb_bench/backend/clients/__init__.py` | 4 edições | +12 | enum `DB` (l.34 vizinhança), `init_cls` (l.117), `config_cls` (l.335), `case_config_cls` (l.576) |
| `vectordb_bench/cli/vectordbbench.py` | 1 edição | +2 | import + `cli.add_command` |
| `pyproject.toml` | 1 edição | +1 | extra `theodb` |
| `tests/test_theodb.py` | novo | ~180 | testes do cliente |

Nenhum arquivo de núcleo (`runner/`, `dataset.py`, `metric.py`, `models.py`) é tocado — a disciplina de diff
mínimo da Política de Fork (D3).

**Repositório `theo-db`:**

| Arquivo | Estado | Papel |
|---|---|---|
| `benchmarks/vectordbbench/docker-compose.yml` | novo | os dois bancos, PG18 dos dois lados |
| `benchmarks/vectordbbench/README.md` | novo | como reproduzir, do zero |
| `wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md` | novo | o resultado, com recall ao lado de cada latência |
| `wiki/` (conceitos OKF) | atualização | `Measurement` da corrida; `Technology` do arnês |
| `CHANGELOG.md` | edição | `[Unreleased]` |

### Current callers / dependents

O contrato do arnês, medido em `backend/clients/api.py`:

| Símbolo | Linha | O que o arnês chama |
|---|---|---|
| `VectorDB.__init__` | 202 | uma vez por caso; o objeto é **copiado para vários processos** |
| `VectorDB.init()` | 225 | contextmanager, por processo/thread |
| `VectorDB.insert_embeddings` | 325 | lotes de 5.000 |
| `VectorDB.search_embedding` | 347 | caminho quente, medido |
| `VectorDB.optimize` | 367 | entre carga e busca; entra no `load_duration` |
| `DBConfig.to_dict` | 126 | kwargs de conexão |
| `DBCaseConfig.index_param` / `search_param` | 146/150 | parâmetros do caso |

O registro tem **quatro** despachos em `backend/clients/__init__.py` e **um** import de CLI em
`cli/vectordbbench.py:33`. `tests/test_db_client_resolution.py` é parametrizado sobre `list(DB)` — acrescentar
o membro do enum estende esse teste automaticamente.

### Domain glossary

| Termo | Significado aqui |
|---|---|
| **recall@k** | fração dos k vizinhos verdadeiros que a busca aproximada devolveu. É a única métrica que torna uma latência interpretável |
| **`ef_search`** | tamanho da lista de candidatos na descida do grafo HNSW. Sobe recall, desce QPS |
| **`ef_construction` / `m`** | parâmetros de **construção** do grafo. No TheoDB são constantes (`build.rs:22-23`) |
| **reloption** | opção de índice em `WITH (…)`, gravada em `pg_class.reloptions` |
| **formato de fio binário** | codificação do `vector` no protocolo estendido: `int16 dim`, `int16` reservado, `float4[]` |
| **caso (case)** | dataset + tamanho + filtro + parâmetros, no vocabulário do arnês |
| **`theodb-hnsw`** | o comando de CLI que este plano acrescenta |

### Architecture boundaries affected

```
VectorDBBench (fork)                          theo-db (este repo)
┌──────────────────────────────┐
│ runner/ (núcleo — INTOCADO)  │
│        ↕ porta VectorDB      │
│ clients/theodb/  ← NOVO      │              benchmarks/vectordbbench/
│   config.py  (Template M.)   │                docker-compose.yml  ← NOVO
│   theodb.py  (Adapter)       │  ──psycopg──▶  theodb:b034      (PG18)
│   cli.py     (Command)       │  ──psycopg──▶  pgvector:pg18    (PG18)
└──────────────────────────────┘              wiki/benchmarks/  ← o artefato
```

A fronteira que importa: o cliente **só** implementa a porta `VectorDB`. Nada do núcleo do arnês é
subclassado, monkey-patched ou reescrito — se fosse, o fork deixaria de ser rebaseável e a saída prevista
pela D3 ("morre quando o upstream aceitar") viraria ficção.

## Prior Art

| Fonte | O que ensina | Onde |
|---|---|---|
| Oportunidade do B-035 | as medições que sustentam todo este plano | `.claude/knowledge-base/discoveries/opportunities/b035-vectordbbench-client-opportunity.md` |
| Cliente `pgvectorscale` do arnês | a forma mais limpa de um cliente PG: config abstrata + concreta, CLI por `TypedDict` | `clients/pgvectorscale/config.py:41-110`, `cli.py:96-122` |
| Cliente `pgvector` do arnês | a ordem correta `connect → register_vector → cursor`, e o `COPY BINARY` | `clients/pgvector/pgvector.py:91-100`, `442-505` |
| Ciclo B-034 | a lição central: um botão aceito em silêncio produz medição plana que parece válida | `.claude/knowledge-base/releases/b034-release.md` |
| ADR-0035 / M73 | o veredito medido do próprio projeto sobre QPS vetorial vs ScaNN | `wiki/decisions/0035-m73-northstar-vector-verdict.md` |
| Regra 5 do projeto | nenhuma alegação de performance sem artefato reproduzível | `CLAUDE.md` |
| Política de Fork D3 | diff mínimo, upstream-first, saída declarada | `CLAUDE.md` |

## Coverage Matrix

| # | Afirmação do Goal | Tarefa(s) |
|---|---|---|
| G1 | Existe um cliente `theodb` em fork de diff mínimo, e o registro resolve | T1.1 |
| G2 | O cliente executa contra o TheoDB real: carga, índice, busca | T1.2 |
| G3 | Parâmetro de build não honrado **falha alto**, com erro tipado | T1.3 |
| G4 | As três métricas mapeiam para operador e opclass corretos | T1.4 |
| G5 | Instalável por terceiro a partir de checkout limpo | T1.5 |
| G6 | Corrida real com recall, TheoDB e pgvector no **mesmo PG18** | T1.6 |
| G7 | O artefato publica o que a corrida **não** cobre | T1.7 |

Cobertura: **7/7**.

## ADRs

### D1 — O cliente usa os nomes NATIVOS de GUC, não os aliases pgvector

**Decisão.** `session_param()` emite `theodb_hnsw.ef_search`, não `hnsw.ef_search`.

**Alternativas consideradas.**

1. *Usar `hnsw.ef_search` (o alias).* Exercitaria a superfície de compatibilidade dentro do próprio
   benchmark. **Rejeitada:** o alias tem regra de precedência (B-034 — o nome específico vence), então uma
   corrida que o use mede o alias **e** a precedência, não o banco. Um benchmark deve ter o menor número de
   variáveis entre o botão e o efeito. A alegação de drop-in já tem prova própria: os seis testes do B-034
   mais a varredura medida na descoberta deste item.
2. *Deixar configurável por flag.* **Rejeitada:** YAGNI, e uma flag que muda qual GUC a corrida usa é
   exatamente o tipo de knob que torna dois resultados incomparáveis sem que a tabela mostre.

**Consequência.** Se alguém quiser medir o caminho do alias, escreve um caso que o faça — e diz que é isso
que está medindo.

### D2 — Parâmetro de build não honrado falha na CONSTRUÇÃO DA CONFIG, não no `CREATE INDEX`

**Decisão.** `TheoDBHNSWConfig` valida `m` e `ef_construction` num `@model_validator` do pydantic. Valor que
o TheoDB não honra levanta `UnsupportedBuildParameterError`, com o nome do parâmetro, o valor pedido, o valor
honrado e o item de backlog ([[B-036]]).

**Alternativas consideradas.**

1. *Ignorar em silêncio e rodar.* **Rejeitada com prejuízo.** É literalmente o defeito que o B-034 acabou de
   consertar, uma camada acima: a corrida completaria, a tabela diria `ef_construction=200`, e o índice teria
   sido construído com 64. Uma medição errada que parece certa é pior que medição ausente.
2. *Deixar o `CREATE INDEX` falhar sozinho* (o PostgreSQL já dá `unrecognized parameter "m"`). **Rejeitada:**
   falha tarde — depois do download do dataset e da carga, que é a parte cara —, e a mensagem do PostgreSQL
   não diz o que o usuário deveria fazer.
3. *Traduzir para os knobs equivalentes do TheoDB.* **Rejeitada:** não existem. `m` é constante de compilação
   e `ef_construction` só se move por variável de ambiente do servidor (`build.rs:30-36`), que o cliente não
   alcança.

**Consequência.** Um caso padrão do arnês que varra `m` **não roda** contra o TheoDB, e isso aparece como
erro na primeira linha. É a resposta honesta enquanto [[B-036]] estiver aberto.

**Aceitar o que honramos:** `m=16` e `ef_construction=64` passam, porque são exatamente o que o TheoDB faz —
e, por coincidência medida, exatamente os defaults do pgvector. A comparação padrão é maçã-com-maçã.

### D3 — Nenhum passo de manutenção assimétrico

**Decisão.** `optimize()` faz o que o cliente `pgvector` faz: derruba e recria o índice. Sem `VACUUM`, sem
`ANALYZE`, sem `maintenance_work_mem` inflado só do nosso lado.

**Alternativas consideradas.**

1. *Acrescentar `VACUUM ANALYZE` depois da carga.* Melhoraria nossos números. **Rejeitada:** o cliente
   `pgvector` não faz, e o tempo de `optimize` entra no `load_duration` medido. Otimizar um lado da
   comparação e publicar a tabela é fabricação com passos extras.
2. *Acrescentar dos dois lados* (patch também no cliente pgvector). **Rejeitada:** aumentaria o diff do fork
   e mudaria o comportamento do cliente de referência, quebrando a comparabilidade com resultados públicos
   de terceiros que usam o cliente upstream.

### D4 — O cliente no fork, o arnês de reprodução aqui

**Decisão.** O código Python vive só no fork. Este repositório recebe `benchmarks/vectordbbench/` (compose +
README) e o artefato em `wiki/benchmarks/`.

**Alternativas consideradas.**

1. *Manter o cliente aqui e sincronizar para o fork.* **Rejeitada:** duas cópias do mesmo conhecimento
   (violação de DRY), e o "diff mínimo" do fork viraria mentira — ele conteria uma cópia.
2. *Só no fork, nada aqui.* **Rejeitada:** a reprodução exige fixar a **mesma versão de PostgreSQL** dos dois
   lados, e essa é uma decisão nossa sobre a nossa medição. Deixá-la só no fork esconde de quem lê este
   repositório o detalhe que torna a comparação válida.

### D5 — Apenas `NonFilter` na primeira versão

**Decisão.** `supported_filter_types = [FilterOp.NonFilter]`.

**Alternativas consideradas.**

1. *Suportar `NumGE` e `StrEqual` já* (o cliente pgvector suporta). **Rejeitada por YAGNI e por honestidade:**
   o TheoDB tem superfície de filtro (`theodb.enable_vecfilter`, `theodb_ivfflat_label_ops`) que **não foi
   medida** neste ciclo. Declarar suporte sem medir é a alegação-sem-execução que o acervo do projeto
   documenta como classe de defeito.

**Consequência.** Casos com filtro são recusados pelo arnês antes de rodar, via `filter_supported`. Declarado
no artefato.

## Tasks

### T1.1 — O pacote do cliente existe e o registro resolve

#### Why this step

Sem o membro do enum e os quatro despachos, nada mais é alcançável — `DB.TheoDB` não existe e a CLI não tem
comando. É a fundação, e é também o que o teste parametrizado de resolução do upstream passa a cobrir de
graça.

#### TDD

RED — em `tests/test_theodb.py`:

```python
def test_theodb_is_registered_and_resolves():
    from vectordb_bench.backend.clients import DB
    assert DB.TheoDB.value == "TheoDB"
    assert DB.TheoDB.init_cls.__name__ == "TheoDB"
    assert DB.TheoDB.config_cls.__name__ == "TheoDBConfig"
    assert DB.TheoDB.case_config_cls(IndexType.HNSW).__name__ == "TheoDBHNSWConfig"

def test_theodb_cli_command_is_exposed():
    from vectordb_bench.cli.vectordbbench import cli
    assert "theodb-hnsw" in cli.commands
```

GREEN — criar `clients/theodb/{__init__,config,theodb,cli}.py` e as 5 edições de registro.

#### Acceptance criteria

- `pytest tests/test_theodb.py::test_theodb_is_registered_and_resolves` passa
- `pytest tests/test_db_client_resolution.py -k TheoDB` passa (teste do upstream, estendido pelo enum)
- `git diff --stat upstream/main` mostra **0** arquivos alterados sob `vectordb_bench/backend/runner/`,
  `vectordb_bench/backend/dataset.py`, `metric.py`, `models.py`

### T1.2 — O cliente executa contra o TheoDB real

#### Why this step

Um cliente que importa mas não consulta é o defeito `cobertura-alegada-sem-execucao` com outro nome. O
caminho quente — `COPY BINARY`, `CREATE INDEX`, `ORDER BY <->` preparado e binário — precisa rodar contra o
banco, não contra um mock.

#### TDD

RED — teste de integração, pulado sem contêiner:

```python
@pytest.mark.skipif(not _theodb_reachable(), reason="TheoDB container not running")
def test_client_loads_and_searches_against_live_theodb():
    db = TheoDB(dim=128, db_config=cfg.to_dict(), db_case_config=TheoDBHNSWConfig(metric_type=MetricType.L2),
                collection_name="t_b035", drop_old=True)
    with db.init():
        count, err = db.insert_embeddings(vectors.tolist(), list(range(N)))
        assert err is None and count == N
        db.optimize()
        got = db.search_embedding(query.tolist(), k=10)
        assert len(got) == 10
        assert len(set(got) & truth) >= 5        # recall real, não apenas "devolveu 10 linhas"
```

GREEN — implementar `TheoDB` com `_create_connection` na ordem `connect → register_vector → cursor` (a ordem
que a descoberta provou obrigatória).

#### Acceptance criteria

- `pytest tests/test_theodb.py -k live` sai com exit 0 e reporta `1 passed` com o contêiner de pé, e `1 skipped` sem ele — nunca `failed`
- a asserção é `assert len(set(got) & truth) >= 5` — interseção com a verdade-terreno, nunca `assert len(got) == 10` sozinho
- `EXPLAIN` da consulta do cliente mostra `Index Scan`, verificado dentro do teste

### T1.3 — Parâmetro de build não honrado falha alto

#### Why this step

É o coração deste plano e a razão de ele existir na forma que tem. Sem esta tarefa, o cliente reproduz o
defeito do B-034: aceita o botão, ignora, e a tabela publicada mente sobre o que foi construído.

#### TDD

RED:

```python
def test_unsupported_m_is_refused_at_config_time():
    with pytest.raises(UnsupportedBuildParameterError) as e:
        TheoDBHNSWConfig(metric_type=MetricType.L2, m=32)
    assert "m" in str(e.value) and "32" in str(e.value) and "16" in str(e.value)
    assert "B-036" in str(e.value)

def test_unsupported_ef_construction_is_refused_at_config_time():
    with pytest.raises(UnsupportedBuildParameterError):
        TheoDBHNSWConfig(metric_type=MetricType.L2, ef_construction=200)

def test_honored_values_are_accepted():
    c = TheoDBHNSWConfig(metric_type=MetricType.L2, m=16, ef_construction=64)
    assert c.index_param()["options"] == {}     # nada é repassado ao WITH

def test_none_is_accepted_and_omitted():
    assert TheoDBHNSWConfig(metric_type=MetricType.L2).index_param()["options"] == {}

def test_create_index_emits_no_with_clause():
    # o SQL gerado não contém "WITH (" — provado sobre a string, não sobre a intenção
    assert "WITH (" not in _rendered_create_index_sql()
```

GREEN — `UnsupportedBuildParameterError(ValueError)` + `@model_validator(mode="after")` comparando contra as
constantes `THEODB_HNSW_M = 16` / `THEODB_HNSW_EF_CONSTRUCTION = 64`, documentadas com o `file:line` da fonte
Rust de onde vieram.

#### Acceptance criteria

- pedir `m` ou `ef_construction` diferente do honrado levanta erro **tipado** antes de qualquer conexão
- `assert all(t in str(exc) for t in ('m', '32', '16', 'B-036'))` — a mensagem contém parâmetro, valor pedido, valor honrado e o item
- o valor honrado e o `None` passam
- o SQL de `CREATE INDEX` gerado **não tem** cláusula `WITH`, provado por asserção sobre a string

### T1.4 — As três métricas mapeiam para operador e opclass corretos

#### Why this step

Um erro de mapeamento aqui não quebra nada visivelmente: o índice é criado, a consulta roda, e o recall
despenca de um jeito que se parece com "o índice é ruim". É a classe de defeito mais cara de diagnosticar
depois — e a mais barata de cobrir agora.

#### TDD

RED — tabela de verdade explícita:

```python
@pytest.mark.parametrize("metric,opclass,op", [
    (MetricType.L2,     "vector_l2_ops",     "<->"),
    (MetricType.COSINE, "vector_cosine_ops", "<=>"),
    (MetricType.IP,     "vector_ip_ops",     "<#>"),
])
def test_metric_maps_to_opclass_and_operator(metric, opclass, op):
    c = TheoDBHNSWConfig(metric_type=metric)
    assert c.index_param()["metric"] == opclass
    assert c.search_param()["metric_fun_op"] == op

def test_unknown_metric_is_refused():
    with pytest.raises(ValueError):
        TheoDBHNSWConfig(metric_type=MetricType.HAMMING).index_param()
```

GREEN — dicionário literal de mapeamento e um `raise` no caminho desconhecido.

#### Acceptance criteria

- os três pares batem com o que `pg_opclass` e `pg_operator` mostram no banco (medido na descoberta)
- `pytest.raises(ValueError)` em `TheoDBHNSWConfig(metric_type=MetricType.HAMMING).index_param()` — nunca um default silencioso
- `need_normalize_cosine()` devolve `False` — o TheoDB tem cosseno de verdade, normalizar seria mudar o dado

### T1.5 — Instalável por terceiro a partir de checkout limpo

#### Why this step

O DoD do item exige que outra pessoa consiga rodar. Um cliente que só funciona no diretório de quem escreveu
não é instrumento, é anotação.

#### TDD

RED — verificação executável em ambiente virgem:

```bash
uv venv --python 3.11 /tmp/vdbb-clean && . /tmp/vdbb-clean/bin/activate
uv pip install "vectordb-bench[theodb] @ git+https://github.com/usetheoai/VectorDBBench@theodb"
vectordbbench theodb-hnsw --help      # deve listar as opções, exit 0
```

#### Acceptance criteria

- instalação em venv limpo, sem o checkout local no `PYTHONPATH`
- `vectordbbench theodb-hnsw --help` sai 0 e lista `--host`, `--db-name`, `--ef-search`
- o extra `theodb` **não acrescenta dependência nova**: reusa `psycopg`, `psycopg-binary`, `pgvector`, que o
  extra `pgvector` já declara (degrau 4 da parsimony ladder)

### T1.6 — Corrida real com recall, nos dois bancos, no mesmo PG18

#### Why this step

É o produto final. E a igualdade de versão do PostgreSQL é o detalhe que decide se a tabela mede
TheoDB × pgvector ou PG18 × PG16 — o compose do upstream fixa `pgvector/pgvector:pg16`.

#### TDD

RED — a corrida é o teste; a falsificação é a assimetria:

```bash
docker compose -f benchmarks/vectordbbench/docker-compose.yml up -d
# verificação de pré-condição que FALHA a corrida se as versões divergirem
test "$(psql_theodb -tAc 'SHOW server_version_num')" -eq "$(psql_pgv -tAc 'SHOW server_version_num')"
vectordbbench theodb-hnsw   --case-type Performance1536D50K --k 10 ...
vectordbbench pgvectorhnsw  --case-type Performance1536D50K --k 10 ...
```

#### Acceptance criteria

- as duas corridas saem com `exit code 0` e o JSON de resultado contém as chaves `recall` e `qps` — `jq -e '.recall and .qps'` sai 0
- a igualdade de `server_version_num` é verificada por script, não por inspeção
- o dataset é o mesmo objeto nas duas corridas: `sha256sum` dos arquivos do cache local `equals` entre as duas execuções
- se qualquer corrida sair com `exit code` diferente de 0, o registro grava o comando e o stderr — `grep -q 'FALHOU' wiki/benchmarks/b035-*.md` acha a entrada, e nenhuma tabela parcial é publicada

### T1.7 — O artefato publica o que a corrida NÃO cobre

#### Why this step

O `theodb_bench` removido tinha teste de significância pareada (Smucker/Allan/Carterette). Este arnês **não
tem**. Publicar uma diferença de QPS sem dizer isso convida a leitura de que a diferença é significativa,
quando ninguém testou.

#### TDD

RED — verificação sobre o artefato:

```python
def test_artifact_declares_its_limits():
    md = Path("wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md").read_text()
    for required in ("significância", "m / ef_construction", "ivfflat", "halfvec", "50.000"):
        assert required in md
    assert "recall" in md.split("QPS")[0].lower() or "recall" in md   # recall nunca ausente
```

#### Acceptance criteria

- o artefato traz recall ao lado de cada número: `grep -c 'recall' wiki/benchmarks/b035-*.md` `>= 1` e nenhuma linha de tabela com `qps` sem `recall` na mesma linha
- declara: sem significância pareada; escala 50K; sem varredura de `m`/`ef_construction` ([[B-036]]); sem
  `ivfflat` ([[B-037]]); sem `halfvec` ([[B-038]]); só `NonFilter`
- o comando exato de reprodução está no artefato: `grep -q 'vectordbbench theodb-hnsw' wiki/benchmarks/b035-*.md` sai 0
- conceito `Measurement` na wiki, atualizando o existente se houver classe igual

## Failure scenarios

O cliente faz I/O externo (rede ao PostgreSQL; o arnês baixa dataset de S3). Cenários cobertos:

| Cenário | Comportamento exigido | Onde |
|---|---|---|
| Banco inalcançável na construção | erro de conexão propaga com host/porta na mensagem; **nunca** captura-e-segue | T1.2 |
| Conexão cai no meio do `COPY` | `insert_embeddings` devolve `(0, exc)` — o contrato do arnês —, com log do erro | T1.2 |
| `CREATE EXTENSION vector` falha (extensão ausente) | falha alto, dizendo que a imagem não tem a extensão | T1.2 |
| Dataset S3 indisponível | é do arnês, não nosso; a corrida falha e o registro diz que falhou no download | T1.6 |
| `CREATE INDEX` rejeita opção | não pode acontecer (T1.3 barra antes); se acontecer, é bug e falha alto | T1.3 |
| Versões de PostgreSQL divergem | a pré-condição da corrida falha **antes** de medir | T1.6 |

## Concurrency tests

O arnês executa busca com múltiplos processos/threads e **copia** a instância do cliente
(`api.py:168` — "the object will be copied into multiple processes"; `thread_safe` em `api.py:188`).

| Verificação | Como |
|---|---|
| `thread_safe = False` declarado | asserção direta — a conexão psycopg não é compartilhável entre threads |
| A instância é copiável quando não tem conexão viva | `copy.deepcopy(db)` fora do `init()` não levanta — é exatamente o defeito #756 que o `tests/test_pgvector.py` do upstream documenta |
| `init()` por thread abre e fecha conexão própria | duas threads em `init()` simultâneo completam sem erro de cursor |

O teste de deepcopy não é decorativo: o upstream tem um arquivo de teste inteiro dedicado a essa regressão no
cliente pgvector, e nosso cliente tem a mesma estrutura de conexão.

## Dependencies

Nenhuma dependência nova. Degrau 4 da parsimony ladder — reusar o que já está declarado.

| Pacote | Versão | Origem | Licença | Por que já está aqui |
|---|---|---|---|---|
| `psycopg` | 3.3.4 | extra `pgvector` do upstream | LGPL-3.0 (cliente, não distribuído no nosso pacote) | driver do cliente pgvector |
| `psycopg-binary` | 3.3.4 | idem | LGPL-3.0 | idem |
| `pgvector` (python) | 0.5.0 | idem | MIT | adaptadores do tipo `vector` |
| `numpy` | 2.4.6 | núcleo do arnês | BSD-3 | já é dependência direta |

**Sobre o D1 (licença):** estas são dependências de um **arnês de medição**, não da distribuição do TheoDB.
Nenhuma entra no pacote do produto. O `psycopg` é LGPL e isso é irrelevante aqui pela mesma razão — o gate D1
governa o que distribuímos. O **fork** é MIT, como o upstream.

## Drawbacks & Risks

| # | Risco | Probabilidade | Mitigação |
|---|---|---|---|
| R1 | **Rebase do fork.** O upstream empurrou 2026-08-11 e é ativo. Um refactor do contrato `VectorDB` quebra o cliente | alta ao longo do tempo | Diff mínimo (D4) e zero toque no núcleo; a saída está declarada — o fork morre quando o PR for aceito |
| R2 | **A tabela vai mostrar o que o M73 já mediu.** Entrar num arnês com cliente `alloydb` convida comparação com a âncora, e superioridade de QPS sobre ScaNN foi medida como não-alcançável | certa | Não é razão para não fazer; é razão para o artefato dizer o posicionamento honesto — paridade de recall, memória, abertura — antes de alguém ler a tabela sozinho |
| R3 | **Escala 50K é pequena.** É o menor caso padrão; a essa escala mede-se bastante o cliente Python | alta | Declarado no artefato (T1.7). A escala maior é trabalho seguinte, não alegação escondida |
| R4 | **Sem significância pareada.** O arnês não tem; o `theodb_bench` removido tinha | certa | Declarado (T1.7). Qualquer alegação comparativa precisa dela por cima, e o artefato diz isso |
| R5 | **Três lacunas cortam a superfície mensurável:** sem `m`/`ef_construction`, sem `ivfflat`, sem `halfvec` | certa | São [[B-036]], [[B-037]], [[B-038]]. O cliente recusa alto (D2) em vez de fingir |
| R6 | **A imagem do produto nunca foi publicada** (10/10 falhas do publish). Um terceiro precisaria compilar | alta | É por isso que o PR upstream fica para depois — está escrito no item. O fork é utilizável hoje por quem tem a imagem local |
| R7 | Este repositório não roda os testes Python do fork; `/code-quality` audita Rust | certa | Os testes do cliente rodam no fork e o resultado entra no relatório de review, com a limitação dita |

## Unresolved Questions

- Q1 — **`SET "theodb_hnsw.ef_search" = N` com identificador citado funciona?** O mecanismo `session_options`
  do arnês envolve o nome em `sql.Identifier`, o que produz aspas em torno de um nome com ponto. **Resolver
  por execução em T1.2**, não por leitura — e, se não funcionar, o cliente emite o `SET` com literal próprio
  em vez de usar o mecanismo do upstream.
- Q2 — **`Performance1536D50K` é o menor caso padrão, mas é OpenAI dim 1536.** Existe SIFT dim 128 (500K),
  que é o dataset das medições históricas do projeto (M33/M45). Vale rodar os dois? Em aberto: a primeira
  corrida usa o menor caso padrão; a segunda, se houver, usa SIFT para conectar com o histórico.
- Q3 — **O upstream aceitaria o cliente sem imagem pública?** Não bloqueia este plano — o PR está fora do
  escopo por decisão registrada no item — mas define se a saída da D3 é alcançável. Sem resposta hoje.
