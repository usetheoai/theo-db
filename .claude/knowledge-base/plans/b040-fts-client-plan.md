---
slug: b040-fts-client
items: [B-040]
date: 2026-08-12
branch: workspace
---

# O pilar lexical entra no arnês, com o handicap declarado antes do número

## Goal

Acrescentar suporte a full-text search ao cliente `theodb` do VectorDBBench — quatro métodos na classe que
já existe, sem tocar o registro — de modo que (a) uma corrida real do `FTSBm25Performance` complete e emita
**recall, MRR e NDCG** ao lado de QPS, (b) o cliente **recuse alto** qualquer parâmetro de caso que o TheoDB
não honre, e (c) o artefato publicado declare o handicap medido — **sem stemming, sem operadores de consulta,
`k1`/`b` não configuráveis** — antes de qualquer comparação com motores que os têm.

## Baseline Context

### Files that will be touched

**Fork `usetheoai/VectorDBBench@theodb`** (já existe, criado no B-035, base `5d0d314`):

| Arquivo | Estado | LoC estimadas | Papel |
|---|---|---|---|
| `vectordb_bench/backend/clients/theodb/theodb.py` | edição | ~+110 | 4 métodos do contrato FTS + o SQL de build/search |
| `vectordb_bench/backend/clients/theodb/config.py` | edição | ~+40 | `TheoDBFTSConfig` e a recusa de parâmetros não honrados |
| `tests/test_theodb_fts.py` | novo | ~150 | testes do caminho FTS |

**Zero** pontos de registro novos: o arnês descobre FTS por `supports_full_text_search()` na classe, não por
entrada no enum — verificado em `elastic_cloud.py:69`.

**Repositório `theo-db`:**

| Arquivo | Estado | Papel |
|---|---|---|
| `benchmarks/vectordbbench/run-fts.sh` | novo | a corrida FTS, com os mesmos gates do `run.sh` |
| `wiki/benchmarks/b040-theodb-fts-msmarco.md` | novo | o resultado, com o handicap no topo |
| `benchmarks/vectordbbench/results/` | artefatos | logs + JSON da corrida |
| `CHANGELOG.md` | edição | `[Unreleased]` |

### Current callers / dependents

| Símbolo | Onde | O que o arnês espera |
|---|---|---|
| `supports_full_text_search()` | `api.py:273` | classmethod; `False` por padrão — retornar `True` habilita o caso |
| `has_text_field()` | `api.py:250` | `True` quando o backend guarda o texto bruto |
| `insert_documents(texts, doc_ids, **kw)` | `api.py:281` | devolve `(inseridos, erro|None)`; conta os que entraram mesmo em falha parcial |
| `search_documents(query, k, payload_profile, **kw)` | `api.py:302` | devolve **IDs de documento ranqueados**, `list[str]` |
| `optimize(data_size)` | `api.py:367` | entre carga e busca; o tempo entra no build medido |
| `concurrent_runner.py` | runner | é quem chama `insert_documents` / `search_documents` |

Do nosso lado, medido: `bm25_build(index_id, table, id_col, text_col)` devolve a contagem indexada;
`bm25_search(index_id, query, k)` devolve `(id, score)` ordenado por score decrescente.

### Domain glossary

| Termo | Significado aqui |
|---|---|
| **BM25** | função de ranqueamento por frequência de termo e frequência inversa de documento, com saturação (`k1`) e normalização por comprimento (`b`) |
| **NDCG@k** | ganho cumulativo descontado normalizado — mede se os documentos relevantes vieram no topo, com julgamento humano graduado |
| **MRR** | posição recíproca média do primeiro documento relevante |
| **qrels** | julgamentos de relevância do dataset; é o que torna NDCG e MRR possíveis |
| **stemming** | reduzir palavras ao radical (`jumping` → `jump`). **O TheoDB não faz** |
| **`index_id`** | identificador do índice lexical no TheoDB; índices distintos coexistem (medido) |
| **product-default** | corrida sem ajustar `k1`/`b` — no nosso caso não é escolha, é a única opção |

### Architecture boundaries affected

```
VectorDBBench (fork)                       TheoDB
┌────────────────────────────┐
│ runner/concurrent_runner   │  (núcleo — INTOCADO)
│        ↕ porta VectorDB    │
│ clients/theodb/theodb.py   │  ──SQL──▶  bm25_build(id, tabela, col_id, col_texto)
│   + 4 métodos FTS          │  ──SQL──▶  bm25_search(id, consulta, k)
│ clients/theodb/config.py   │
│   + TheoDBFTSConfig        │
└────────────────────────────┘
```

A mesma fronteira do B-035: só a porta `VectorDB` é implementada. O acréscimo é **dentro da classe que já
existe** — nenhum arquivo do núcleo, nenhum ponto de registro.

## Prior Art

| Fonte | O que ensina | Onde |
|---|---|---|
| Oportunidade do B-040 | as medições que sustentam este plano | `.claude/knowledge-base/discoveries/opportunities/b040-fts-client-opportunity.md` |
| Ciclo B-035 | o cliente, os gates do runner, e a lição do recall não casado | `.claude/knowledge-base/releases/b035-release.md` |
| Cliente Elastic do arnês | como os 4 métodos FTS se encaixam numa classe existente | `clients/elastic_cloud/elastic_cloud.py:69,72,177,281` |
| [[B-004]] + `m186-lexical-ndcg-scifact-verdict` | nDCG 0,6269 contra 0,3016 do `ts_rank_cd` no SciFact; e a nota sobre multi-termo que a descoberta corrigiu | `wiki/benchmarks/m186-lexical-ndcg-scifact-verdict.md` |
| Regra 5 | nenhuma alegação de performance sem artefato reproduzível | `CLAUDE.md` |
| Política de Fork D3 | diff mínimo, saída declarada | `CLAUDE.md` |

## Coverage Matrix

| # | Afirmação do Goal | Tarefa(s) |
|---|---|---|
| G1 | Os 4 métodos do contrato FTS existem e o arnês os enxerga | T1.1 |
| G2 | Carga, build e busca funcionam contra o TheoDB real | T1.2 |
| G3 | A busca devolve **IDs ranqueados**, e o ranking é o do BM25 | T1.3 |
| G4 | Parâmetro de caso não honrado **falha alto** | T1.4 |
| G5 | Corrida real emitindo recall, MRR e NDCG | T1.5 |
| G6 | O artefato declara o handicap **antes** do número | T1.6 |

Cobertura: **6/6**.

## ADRs

### D1 — Os 4 métodos vão na classe `TheoDB` existente, não numa classe FTS separada

**Decisão.** `supports_full_text_search`, `has_text_field`, `insert_documents` e `search_documents` são
acrescentados a `TheoDB`.

**Alternativas consideradas.**

1. *Classe `TheoDBFTS` própria + entrada no enum `DB`.* **Rejeitada:** o arnês descobre FTS pelo método na
   classe, não pelo enum — verificado no Elastic, que tem uma entrada só. Uma entrada extra criaria dois
   `DB` para o mesmo banco e o `test_db_client_resolution` parametrizado passaria a cobrir um alvo fantasma.
2. *Herança `TheoDBFTS(TheoDB)`.* **Rejeitada por YAGNI:** não há comportamento a especializar; a config já
   separa os dois caminhos, e uma subclasse com quatro métodos e nenhum override é indireção sem valor
   (degrau 5 da parsimony ladder).

### D2 — `optimize()` constrói o índice lexical; a carga não constrói

**Decisão.** `insert_documents` faz só `COPY` para a tabela. `optimize()` chama `bm25_build`.

**Alternativas consideradas.**

1. *Construir a cada lote em `insert_documents`.* **Rejeitada:** `bm25_build` é uma construção completa sobre
   a tabela (medido: reexecutar reindexa tudo), então construir por lote seria O(n²) em trabalho e o tempo
   de build medido pelo arnês ficaria sem sentido.
2. *Construir na primeira busca, preguiçosamente.* **Rejeitada:** o arnês **mede** o tempo de build como
   métrica própria; escondê-lo dentro da primeira consulta reporta build zero e infla a latência da primeira
   busca. Seria fabricar dois números ao mesmo tempo.

### D3 — Parâmetros de BM25 não são aceitos, porque não são honrados

**Decisão.** Se o caso pedir `k1` ou `b`, `TheoDBFTSConfig` levanta `UnsupportedBuildParameterError` —
reusando a classe do B-035, com a mesma mensagem e o item de rastreio.

**Alternativas consideradas.**

1. *Aceitar e ignorar.* **Rejeitada com prejuízo:** é o defeito do B-034 e do B-035 numa terceira camada. O
   arnês avisa que os motores diferem em `k1`/`b` e recomenda declarar; aceitar e ignorar publicaria uma
   comparação "parametrizada" que não foi.
2. *Expor `k1`/`b` como GUC no TheoDB.* Trabalho de produto, não deste item. Fica registrado como item novo
   se a corrida mostrar que importa.

### D4 — A ausência de stemming é declarada no artefato, não compensada no cliente

**Decisão.** O cliente **não** faz stemming do lado Python antes de mandar a consulta.

**Alternativas consideradas.**

1. *Stemmar a consulta no cliente (`nltk`/`snowball`).* **Rejeitada, e é a rejeição mais importante deste
   plano:** stemmar a consulta sem stemmar o índice piora o casamento, não melhora. E se stemássemos os dois,
   o benchmark mediria o cliente Python, não o motor — publicaríamos um número que a instalação real do
   TheoDB não reproduz. Um benchmark que mede o adaptador é pior que nenhum.
2. *Não mencionar.* **Rejeitada:** a comparação é contra Elastic e OpenSearch, cujos analisadores padrão
   stemmizam. Omitir é deixar o leitor atribuir ao motor uma diferença que é de pré-processamento.

## Tasks

### T1.1 — Os 4 métodos existem e o arnês os enxerga

#### Why this step

Sem `supports_full_text_search() == True` o caso FTS nem é oferecido ao cliente — o arnês o filtra antes de
começar. É a fundação e é barata de verificar.

#### TDD

RED — em `tests/test_theodb_fts.py`:

```python
def test_theodb_declares_full_text_support():
    from vectordb_bench.backend.clients.theodb.theodb import TheoDB
    assert TheoDB.supports_full_text_search() is True
    assert TheoDB.supports_document_payload_profile(TheoDB, PayloadProfile.TEXT) is True
```

GREEN — os quatro métodos em `TheoDB`.

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- `pytest tests/test_theodb_fts.py::test_theodb_declares_full_text_support` sai com exit 0
- `git diff --stat upstream/main` mostra **0** arquivos alterados sob `vectordb_bench/backend/runner/` e
  **0** linhas novas em `vectordb_bench/backend/clients/__init__.py`
- `make lint` do upstream continua `All checks passed`

### T1.2 — Carga, build e busca contra o TheoDB real

#### Why this step

O contrato pode estar satisfeito por assinaturas e o caminho não funcionar. O que o arnês vai cronometrar é
`COPY` → `bm25_build` → `bm25_search`, e é isso que precisa rodar contra o banco.

#### TDD

RED — teste de integração, pulado sem contêiner:

```python
@needs_theodb
def test_fts_load_build_and_search():
    client = _fts_client(drop_old=True)
    with client.init():
        n, err = client.insert_documents(TEXTS, DOC_IDS)
        assert err is None and n == len(TEXTS)
        client.optimize(data_size=len(TEXTS))
        got = client.search_documents("lazy dog", k=5)
        assert got, "busca vazia após build"
        assert set(got) <= set(DOC_IDS)
```

GREEN — `insert_documents` com `COPY … FROM STDIN`, `optimize` chamando `bm25_build`, `search_documents`
chamando `bm25_search`.

#### Concurrency tests

As buscas deste teste são seriais: uma conexão, um cursor, nenhuma corrida de dados possível. O
**parallel test** que cobre o caminho compartilhado vive em T1.3.

#### Acceptance criteria

- `pytest tests/test_theodb_fts.py -k live` reporta `passed` com contêiner e `skipped` sem ele — nunca `failed`
- os IDs devolvidos são `str` e **todos** pertencem ao conjunto inserido — `assert set(got) <= set(DOC_IDS)`
- `optimize()` chama `bm25_build` exatamente uma vez por corrida, provado por asserção sobre a contagem
  devolvida (`equals` o número de documentos inseridos)

### T1.3 — A busca devolve IDs ranqueados pelo BM25

#### Why this step

Um cliente que devolve os IDs certos **fora de ordem** produz NDCG e MRR errados sem falhar nada. É a classe
de defeito que só aparece na métrica, e a métrica é o produto deste item.

#### TDD

RED — corpus com relevância conhecida por construção:

```python
@needs_theodb
def test_ranking_puts_the_best_document_first():
    # doc A contém os dois termos; doc B contém um; doc C nenhum
    client = _fts_client(drop_old=True)
    with client.init():
        client.insert_documents([DOC_A, DOC_B, DOC_C], ["a", "b", "c"])
        client.optimize()
        assert client.search_documents("lazy dog", k=3)[0] == "a"
        assert "c" not in client.search_documents("lazy dog", k=3)

@needs_theodb
def test_k_is_respected():
    assert len(client.search_documents("lazy dog sun search index", k=2)) <= 2
```

#### Concurrency tests

Um **concurrent test** — `test_concurrent_search_documents_agree` — abre duas threads, cada uma com seu
`init()`, e chama `search_documents` com a mesma consulta: as duas devolvem o mesmo topo
(`assert topo_a == topo_b`). O índice lexical é compartilhado entre processos e o B-035 nunca exercitou
esse caminho.

#### Acceptance criteria

- o primeiro ID devolvido é o documento com mais termos casados — `assert got[0] == "a"`
- documento sem nenhum termo **não** aparece — `assert "c" not in got`
- `len(resultado) <= k` para `k` menor que o número de casamentos
- os IDs voltam como `str`, não `int` — o contrato do arnês é `list[str]`

### T1.4 — Parâmetro não honrado falha alto

#### Why this step

Terceira camada da mesma lição (B-034 no scan, B-035 no build vetorial, aqui no ranqueamento). O arnês
documenta que os motores diferem em `k1`/`b`; aceitar e ignorar publicaria uma parametrização que não houve.

#### TDD

RED:

```python
def test_bm25_parameters_are_refused():
    for field, value in (("k1", 1.5), ("b", 0.6)):
        with pytest.raises(UnsupportedBuildParameterError) as exc:
            TheoDBFTSConfig(**{field: value})
        assert field in str(exc.value) and "B-040" in str(exc.value)

def test_defaults_are_accepted():
    assert TheoDBFTSConfig().index_param()["options"] == {}
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- pedir `k1` ou `b` levanta erro **tipado** antes de qualquer conexão
- a mensagem `contains` o nome do parâmetro e o item de rastreio
- a config sem parâmetros passa e não emite opção nenhuma — `equals {}`

### T1.5 — Corrida real com recall, MRR e NDCG

#### Why this step

É o produto. E as três métricas juntas são o que separa este artefato de um teste de vazão: QPS sozinho não
diz se a busca achou o documento certo.

#### TDD

RED — a corrida é o teste; a falsificação é a métrica ausente:

```bash
./benchmarks/vectordbbench/run-fts.sh          # gates de versão e TCP herdados do run.sh
jq -e '.results[0].metrics | .recall and .ndcg and .mrr' <json>   # sai 0 só se as três existirem
```

#### Concurrency tests

A corrida usa o `concurrent_runner` do arnês, que copia o cliente entre processos. O **parallel test**
`test_fts_client_is_deepcopyable_when_idle` garante o pré-requisito: `copy.deepcopy(client)` fora do
`init()` não levanta e `thread_safe` segue `False` — sem isso o runner compartilharia uma conexão psycopg
entre threads, que é corrida de dados garantida.

#### Acceptance criteria

- a corrida sai com `exit code 0` e o JSON tem **recall, ndcg e mrr** não nulos — `jq -e` sai 0
- o tempo de download do dataset via `ir_datasets` é **medido e registrado** (hoje desconhecido)
- se a corrida falhar, `grep -q 'FALHOU' results/run-fts.out` sai 0 e nenhuma tabela é publicada — o gate do B-035 herdado, verificado pelo mesmo `grep "failed to run"` sobre o log
- o `index_id` usado é fixo e documentado, para a corrida ser repetível

### T1.6 — O artefato declara o handicap antes do número

#### Why this step

A comparação é contra motores cujo analisador padrão faz stemming. Publicar NDCG lado a lado sem dizer isso
deixa o leitor atribuir ao motor uma diferença que é de pré-processamento — o mesmo erro de leitura que o
B-035 documentou com o recall não casado, num eixo diferente.

#### TDD

RED — verificação sobre o artefato:

```python
def test_artifact_declares_the_handicap_before_the_numbers():
    md = Path("wiki/benchmarks/b040-theodb-fts-msmarco.md").read_text()
    for token in ("stemming", "k1", "operadores", "significância", "product-default"):
        assert token in md
    assert md.index("stemming") < md.index("NDCG")   # o handicap vem antes do número
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- o artefato `contains` as palavras `stemming`, `k1`, `operadores`, `significância`, `product-default`
- a primeira menção a `stemming` aparece **antes** da primeira menção a `NDCG` — asserção sobre índices
- o comando exato de reprodução está no artefato — `grep -q 'run-fts.sh'` sai 0
- conceito `Measurement` na wiki, atualizando o existente do pilar lexical se houver classe igual

## Failure scenarios

O cliente faz I/O externo (PostgreSQL por psycopg; o arnês baixa dataset via `ir_datasets`).

| Cenário | Comportamento exigido | Onde |
|---|---|---|
| `bm25_build` falha (tabela ausente, coluna errada) | erro propaga com `index_id`, tabela e coluna na mensagem | T1.2 |
| `bm25_search` sobre `index_id` nunca construído | falha alto, não devolve lista vazia — vazio é indistinguível de "nada casou" | T1.2 |
| Conexão cai no meio do `COPY` | `insert_documents` devolve `(0, exc)` — o contrato do arnês | T1.2 |
| `ir_datasets` indisponível | é do arnês; a corrida falha e o registro diz que falhou no download | T1.5 |
| Consulta vazia ou só stopwords | devolve lista vazia sem erro — é resultado legítimo, não falha | T1.3 |
| Endereço não é TheoDB | a checagem de identidade do B-035 já barra na construção | herdado |

## Concurrency tests

O `concurrent_runner` chama `search_documents` de vários processos e **copia** a instância do cliente.

| Verificação | Como |
|---|---|
| `thread_safe = False` continua declarado | asserção direta — herdado do B-035, e o caminho FTS não muda isso |
| Instância copiável sem conexão viva | `copy.deepcopy(client)` fora do `init()` não levanta |
| Buscas FTS concorrentes não corrompem resultado | duas threads em `init()` + `search_documents` devolvem o mesmo topo para a mesma consulta |

A terceira é a que importa aqui: o índice lexical é compartilhado entre processos e o B-035 não exercitou
esse caminho.

## Dependencies

Nenhuma dependência nova no cliente. O caso FTS do arnês exige `ir_datasets`, que é dependência **do
upstream**, não nossa.

| Pacote | Origem | Licença | Nota |
|---|---|---|---|
| `psycopg` / `psycopg-binary` | extra `theodb`, já declarado | LGPL-3.0 | driver; não entra na distribuição do produto |
| `ir_datasets` | núcleo do arnês para casos FTS | Apache-2.0 | baixa MS MARCO da origem, não do S3 da Zilliz |

**Sobre o D1 (licença):** dependências de arnês de medição, não da distribuição do TheoDB. O gate D1 governa
o que distribuímos.

## Drawbacks & Risks

| # | Risco | Probabilidade | Mitigação |
|---|---|---|---|
| R1 | **A tabela vai mostrar o TheoDB atrás em NDCG**, porque Elastic e OpenSearch stemmizam | alta | Declarado no artefato antes do número (T1.6). É informação, não derrota — e é a razão de medir |
| R2 | **Download do MS MARCO via `ir_datasets` pode ser grande e lento** — custo não medido | média | T1.5 mede e registra. Se inviabilizar, o item declara isso em vez de trocar por um corpus mais fácil sem dizer |
| R3 | `bm25_build` medido a 50K em 210 ms pode não escalar linear a 100K | baixa | A corrida mede; o número publicado é o medido, não a extrapolação |
| R4 | **Sem significância pareada** — o arnês não tem | certa | Declarado. Vale igual ao B-035 |
| R5 | Ausência de operadores de consulta pode afetar casos que os usem | baixa | MS MARCO usa consultas em linguagem natural, sem operadores. Declarado mesmo assim |
| R6 | Este repositório não roda os testes Python do fork | certa | Rodam no fork; o review reporta com a limitação dita |
| R7 | O índice lexical sob concorrência não foi exercitado antes | média | É a verificação nova em § Concurrency tests |

## Unresolved Questions

- Q1 — **Qual o custo real de baixar o MS MARCO Small pelo `ir_datasets`?** Desconhecido. Resolver por
  execução em T1.5, e registrar o número mesmo se for inconveniente.
- Q2 — **O `index_id` deve ser fixo ou derivado do nome da coleção?** Fixo é mais simples e repetível;
  derivado evita colisão se duas corridas dividirem o banco. Decidir em T1.2, com o critério de que duas
  corridas simultâneas no mesmo banco não é cenário que o arnês crie.
- Q3 — **Vale rodar também o HotpotQA?** O arnês oferece. Fica em aberto: a primeira corrida usa MS MARCO
  Small, que é o default do caso; um segundo corpus é trabalho seguinte e conecta melhor com o [[B-004]].
