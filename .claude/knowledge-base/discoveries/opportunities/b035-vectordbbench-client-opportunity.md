---
item: B-035
mode: evolve
date: 2026-08-12
verdict: pending
---

# B-035 — O TheoDB já fala o protocolo do arnês; o que falta é o cliente e a honestidade sobre três lacunas

## Corner 1 — Evidence

Tudo abaixo foi medido em 2026-08-12 contra a imagem `theodb:b034` em execução (contêiner `theodb-b035`,
porta 55435) e contra `zilliztech/VectorDBBench@5d0d314` (2026-08-11), com `psycopg 3.3.4`, `pgvector-python
0.5.0`, `numpy 2.4.6`.

### O arnês: licença e contrato

| Fato | Medido |
|---|---|
| Licença | **MIT** — passa o D1 |
| `requires-python` | `>=3.11` (o host tem 3.10.12; corridas via `uv --python 3.11`) |
| Contrato do cliente | `backend/clients/api.py:167` — `VectorDB` ABC: `__init__`, `init()`, `insert_embeddings`, `search_embedding`, `optimize` |
| Config | `DBConfig` (conexão) + `DBCaseConfig` (`index_param`/`search_param`) |
| Pontos de registro | **5** em arquivos upstream + 1 diretório novo (detalhe abaixo) |

Pontos de toque medidos em `backend/clients/__init__.py`: enum `DB` (l.34), `init_cls` (l.117), `config_cls`
(l.335), `case_config_cls` (l.576); mais `cli/vectordbbench.py:33` e o extra em `pyproject.toml`. É um diff
mínimo de verdade — nenhum arquivo de núcleo é reescrito.

### O que JÁ funciona — e é a maior parte

O cliente `pgvector` do arnês usa o caminho **binário** do `pgvector-python`. A pergunta que decidia a
viabilidade era se o nosso `vector` own-code fala esse formato de fio. **Fala.**

| Verificação | Resultado |
|---|---|
| `CREATE EXTENSION vector` | ok |
| `TypeInfo.fetch(conn,'vector')` | ok — oid 16386, array 16611 |
| `register_vector(conn)` + round-trip binário | ok — devolve `Vector([...])` |
| `COPY … FROM STDIN (FORMAT BINARY)` com `set_types(["bigint","vector"])` | **ok — 500 e depois 5.000 linhas** |
| `CREATE INDEX … USING hnsw (embedding vector_l2_ops)` | ok |
| `EXPLAIN` da consulta k-NN | **`Index Scan using e2e_idx`** |
| `SELECT … ORDER BY embedding <-> %s::vector LIMIT %s` com `prepare=True binary=True` | ok |
| `SET hnsw.ef_search = N` | ok (**é o B-034**) |
| Operadores `<->`, `<=>`, `<#>` sobre `vector` | os três existem |
| Opclasses `vector_l2_ops` / `vector_cosine_ops` / `vector_ip_ops` no AM `hnsw` | as três existem |

O caminho ponta a ponta, exercitado como o arnês faz — 5.000 vetores dim 128, carga por `COPY BINARY`,
índice pela sintaxe pgvector, 200 consultas contra verdade-terreno exata calculada fora do banco:

```
carga: 5000 vetores dim128 em 0,26s
índice (sintaxe pgvector, sem WITH): 0,87s
plano: Limit | -> Index Scan using e2e_idx on e2e

 ef_search  recall@10       QPS
        10     0,4915     3474,3
        40     0,6240     2911,4
       100     0,8390     1945,0
       400     1,0000      650,4
```

**Esta curva é a prova em situ do B-034.** Antes dele, `SET hnsw.ef_search` era placeholder inerte e as
quatro linhas teriam recall idêntico — a "curva plana" que o item previu. O `blocked_by` está resolvido.

### O que NÃO funciona — três lacunas, todas de falha alta

| # | Lacuna | Medição | Consequência para o arnês |
|---|---|---|---|
| L1 | `WITH (m=…, ef_construction=…)` | `ERROR: unrecognized parameter "m"` — idem `ef_construction`, `max_connections` | O cliente `pgvector` do arnês **sempre** emite essas opções para HNSW. Um cliente TheoDB não pode repassá-las |
| L2 | AM `ivfflat` | não existe — só `theodb_ivfflat` (`pg_am`). `USING ivfflat` falha | Casos IVF do arnês são inatingíveis pelo nome pgvector |
| L3 | `halfvec` / `sparsevec` | não existem (`pg_type`) | Casos de quantização do arnês são inatingíveis |

As reloptions que o AM **de fato** aceita, lidas da fonte (`theodb_rs/src/am/options.rs:112-196`): `lists`,
`sbq_bits`, `pq_subspaces`, `pq_bits`, `aq_threshold`, `separate_storage`, `refine`, `soar_lambda`,
`rabitq_bits`. Não há `m` nem `ef_construction`.

**O build HNSW é fixo:** `HNSW_M = 16` e `HNSW_EF_CONSTRUCTION = 64` são constantes de compilação
(`theodb_rs/src/am/build.rs:22-23`); o segundo é sobreponível apenas por variável de ambiente do
**servidor** (`THEODB_HNSW_EF_CONSTRUCTION`, `build.rs:30-36`), que um cliente não alcança por sessão.

**A coincidência que salva a comparação:** os defaults do pgvector são exatamente `m=16, ef_construction=64`.
Uma corrida TheoDB × pgvector nos defaults é maçã-com-maçã sem nenhum ajuste. Uma corrida que **varra** `m`
ou `ef_construction` não é executável contra o TheoDB — e é aí que o cliente precisa falhar alto em vez de
ignorar, porque ignorar reproduz exatamente o defeito que o B-034 acabou de consertar, uma camada acima.

### Um erro meu, registrado porque quase virou achado

A primeira sonda reportou falha em `register_vector`, `COPY BINARY` e na consulta binária — o que teria sido
um veredito de "formato de fio incompatível", isto é, B-035 inviável. **Era bug da sonda.** O `Cursor` do
psycopg congela os adaptadores no `Transformer` ao ser criado; eu criei o cursor antes do `register_vector`.
Verificado com controle explícito:

```
cursor ANTES de register_vector  -> FALHA ProgrammingError
cursor DEPOIS de register_vector -> OK Vector([0.0, 1.0, 2.0, 3.0])
```

O cliente `pgvector` do arnês já faz na ordem certa (`pgvector.py:91-100`). Fica registrado porque a falha
era determinística e plausível — reproduziu idêntica nas duas execuções — e "determinístico" não é sinônimo
de "correto".

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `usetheoai/VectorDBBench` (fork novo) | 1 diretório de cliente + 5 pontos de registro + 1 extra no `pyproject`. **Nenhum arquivo de núcleo tocado** |
| `theo-db` — código | **nenhum**. Este item não altera a extensão |
| `theo-db` — artefatos | `wiki/benchmarks/` ganha o resultado; `wiki/` ganha os conceitos `Measurement` |
| Usuários do produto | nenhum efeito — é instrumento, não banco |
| Reputacional | entrar num arnês que tem cliente `alloydb` convida comparação com a âncora do North Star. O veredito medido do próprio projeto (M73, ADR-0035) já diz que superioridade de QPS sobre ScaNN **não é alcançável** por extensão permissiva |
| Descoberto de passagem | L1, L2 e L3 são lacunas do produto, não do arnês — viram itens próprios |

## Corner 4 — Verification

1. `pip install` do fork a partir de checkout limpo instala e a CLI expõe o comando novo.
2. Uma corrida real do arnês contra o TheoDB completa e emite recall + QPS — não apenas "o cliente carregou".
3. A mesma corrida contra `pgvector/pgvector:pg18` — **mesma versão de PostgreSQL**, senão mede PG18 × PG16.
4. Pedir `m` ou `ef_construction` diferente do que o TheoDB honra **falha com erro tipado**, provado por
   teste — nunca é aceito em silêncio.
5. O registro publicado diz o que a corrida não cobre: sem significância pareada (o arnês não tem), sem L2,
   sem L3.

## Reclassificação

`suggested_mode: evolve` mantido. Não é defeito do TheoDB — é ausência de instrumento. As três lacunas que a
medição encontrou **são** defeitos, e por isso saem daqui como itens próprios em vez de ficarem em prosa.
