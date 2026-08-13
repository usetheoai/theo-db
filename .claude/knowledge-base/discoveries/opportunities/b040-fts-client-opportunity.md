---
item: B-040
mode: evolve
date: 2026-08-12
verdict: pending
---

# B-040 — O pilar lexical cabe no arnês; o que falta declarar é o handicap

## Corner 1 — Evidence

Medido em 2026-08-12 contra `theodb:b034` em execução (contêiner `vdbb-theodb`) e contra
`zilliztech/VectorDBBench@5d0d314`.

### O contrato do arnês, e uma assimetria a favor

| Fato | Medido |
|---|---|
| Caso | `FTSBm25Performance` (`cases.py:945`), default MS MARCO Small = **100.000 documentos** (`dataset.py:1173`) |
| Métricas | tempo de build, **recall**, latência serial, QPS — mais MRR e NDCG no registro do caso |
| Contrato | `supports_full_text_search()` (`api.py:273`), `insert_documents(texts, doc_ids)` (`api.py:281`), `search_documents(query, k) -> list[str]` (`api.py:302`), `has_text_field()` (`api.py:250`) |
| Onde vive | **métodos na classe de cliente existente** — é assim que o Elastic faz (`elastic_cloud.py:69,72,177,281`). **Zero entradas novas no registro** |
| Origem do dataset | `ir_datasets`, **não** o S3 da Zilliz (`with_remote_resource: False`) — custo de download ainda **não medido** |

**Nenhum cliente PostgreSQL implementa FTS no arnês.** Os cinco que implementam são Elastic, OpenSearch,
Milvus, Turbopuffer e Vespa. Isso torna o trabalho mais valioso upstream do que o cliente vetorial foi — e
é provavelmente a razão de ninguém o ter feito: exige um motor PG com BM25 próprio.

### A nossa superfície, exercitada

`bm25_build(index_id bigint, "table" text, id_col text, text_col text)` e
`bm25_search(index_id bigint, query text, k integer) -> (id, score)`.

Sobre um corpus de 5 documentos:

| Consulta | Resultado | Leitura |
|---|---|---|
| `dog` | `1:0,883 4:0,845` | termo único ok |
| `lazy dog` | `1:1,767 4:1,691` | **multi-termo OR com scores somados — BM25 correto** |
| `database vector` | `2:1,464 3:1,399` | cada documento casou por um termo distinto |
| `what does the lazy dog do all day` | `4:5,543 1:2,978` | **pergunta natural rankeia o documento certo em primeiro** — é a forma exata das consultas do MS MARCO |
| `zebra` | vazio | termo ausente não inventa resultado |
| `LAZY, Dog!` | `1 4` | pontuação e caixa normalizadas |
| `k=1` sobre consulta ampla | 1 linha | `k` respeitado |

### O que a superfície NÃO tem — medido termo a termo

| Recurso | Resultado | Consequência |
|---|---|---|
| Frase exata `"lazy dog"` | idêntico ao sem aspas | aspas ignoradas, sem semântica de frase |
| `AND` booleano | `lazy AND dog` traz o doc 3 | **`AND` é tratado como termo** e casa documentos que contêm "and" |
| Exclusão `-dog` | idêntico a `lazy dog` | ignorada |
| Prefixo `jump*` | vazio | curinga não suportado |
| **Stemming** | `jumping` não casa `jumps` | **é o handicap que importa** |
| Stopwords | `the` devolve `1 4` | indexadas, não removidas |
| `k1` / `b` do BM25 | **nenhum GUC** | corrida é product-default, e só |

### Escala e o mapeamento com o ciclo do arnês

| Verificação | Medido |
|---|---|
| `bm25_build` sobre **50.000** documentos | **210 ms** |
| `bm25_search` no mesmo índice | **9,8 ms** |
| Reexecutar `bm25_build` no mesmo `index_id` após inserir | reindexa e **vê o documento novo** |
| Dois `index_id` distintos | coexistem e respondem independentemente |

O rebuild no mesmo id é o que torna o mapeamento viável: `insert_documents` faz `COPY` para a tabela,
`optimize()` chama `bm25_build`, `search_documents` chama `bm25_search`. É o mesmo formato de ciclo que o
cliente vetorial já usa e que o arnês espera.

### Uma nota do próprio backlog que a medição derrubou

O [[B-004]] registrava, como achado lateral, que *"a superfície não expõe busca multi-termo"*. **Falso como
escrito** — a tabela acima mostra multi-termo funcionando com scores corretos. O que falta é mais estreito:
operadores e stemming. Corrigido lá por acréscimo, porque a nota errada tornaria este item inviável no papel.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `usetheoai/VectorDBBench@theodb` | **4 métodos** na classe `TheoDB` existente. **Zero** pontos de registro novos — menor que o diff do B-035 |
| `theo-db` — código | **nenhum**. Este item não altera a extensão |
| `theo-db` — artefatos | `wiki/benchmarks/` ganha o resultado; `benchmarks/vectordbbench/` ganha o comando |
| Usuários do produto | nenhum efeito — é instrumento |
| Reputacional | entrar num leaderboard público de full-text convida comparação direta com Elastic e OpenSearch, que **têm stemming**. A tabela vai mostrar isso |
| [[B-004]] | uma corrida aqui avança o DoD aberto dele (generalização além do SciFact) sem substituí-lo |

## Corner 4 — Verification

1. Uma corrida completa do `FTSBm25Performance` emite **recall, MRR e NDCG** ao lado de QPS — nunca QPS só.
2. O cliente **recusa alto** parâmetro de caso que o TheoDB não honre (a decisão D2 do B-035 vale igual).
3. O artefato declara: **sem stemming**, sem operadores de consulta, `k1`/`b` não configuráveis, corrida
   product-default dos dois lados.
4. O gate de "meia comparação não se publica" do `run.sh` continua valendo.
5. O tempo de download do dataset via `ir_datasets` é medido e registrado — hoje é desconhecido.

## Reclassificação

`suggested_mode: evolve` mantido. Não há defeito do TheoDB aqui: a superfície funciona e é rápida. O que
falta é instrumento — e a honestidade sobre um handicap que nenhum código deste item corrige.
