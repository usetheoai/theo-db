---
slug: b040-fts-client
items: [B-040]
date: 2026-08-13
base: cd78ab9
head: 86e1ad9
verdict: READY_TO_MERGE
---

# Review — o pilar lexical medido, e o handicap dito antes da tabela

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Testes verdes | **77/77** no fork (12 FTS + 20 vetorial + 44 resolução parametrizada + 1) |
| 2 | Lint do upstream (`make lint`) | `black --check` + `ruff check` — **All checks passed** |
| 3 | Segredos commitados | **0** |
| 4 | Commit direto em `main` | não — `workspace` |
| 5 | Trailer de coautoria | **0** |
| 6 | `CHANGELOG.md` atualizado | sim |
| 7 | Bundle OKF | **302 conceitos, 0 erros, 0 warnings, 0 links quebrados** |

`/code-quality`: **`FAIL_SOFT`**, Rust auditado, **0 achados HARD**. Os dois caps são de ambiente, e o
[[B-039]] já registra que o `cargo-udeps` passa limpo em 2m07s quando invocado no contêiner pinado. Este
ciclo **não altera uma linha de Rust**.

## Cross-validation — 6 de 6

| # | Afirmação do Goal | Como foi verificada | Resultado |
|---|---|---|---|
| G1 | Os 4 métodos existem e o arnês os enxerga | `test_theodb_declares_full_text_support`, `test_theodb_declares_a_text_field_and_the_text_payload_profile`; `git diff --numstat` sobre o registro | ok — **2 arquivos neste ciclo, zero linhas no registro** |
| G2 | Carga, build e busca contra o TheoDB real | `test_fts_load_build_and_search` + `test_search_before_build_fails_loudly` | ok |
| G3 | Busca devolve IDs ranqueados pelo BM25 | `test_ranking_puts_the_best_document_first` (relevância conhecida por construção), `test_k_is_respected`, `test_query_with_no_matching_term_returns_empty_without_error` | ok |
| G4 | Parâmetro não honrado falha alto | `test_bm25_parameters_are_refused` parametrizado sobre `k1` e `b`, com asserção sobre a mensagem | ok |
| G5 | Corrida real com recall, MRR e NDCG | corrida no droplet; JSON com as três chaves não nulas | ok |
| G6 | O artefato declara o handicap antes do número | asserção sobre índices no markdown | ok — `stemming` em 205, `NDCG` em 288 |

## O resultado

Droplet `g-16vcpu-64gb` (Xeon 8358), **IP 167.172.229.34**, efêmero e destruído — verificado: a listagem
por tag volta vazia e a chave SSH foi removida.

| Métrica | Valor |
|---|---|
| **NDCG@10** | **0,6962** |
| **recall@10** | **0,8025** |
| **MRR** | **0,667** |
| QPS (pico, 60 clientes) | 1.616,4 |
| p99 serial | 4,8 ms |
| Carga de 100.000 documentos | 25,6 s |
| Duração total (com download de 3,9 GB) | 887 s |

O throughput satura em ~20 clientes e fica plano até 80, com p99 crescendo linearmente — teto de vazão, não
degradação.

## Achados

### R-1 — MÉDIO · `bm25_build` exige chave `BIGINT`; os ids do arnês são strings opacas

Medido: uma coluna de id `TEXT` falha com `invalid input syntax for type bigint`. Os ids do MS MARCO são
numéricos, mas os do HotpotQA não — coagir funcionaria num dataset e corromperia o próximo.

Resolvido com chave substituta `GENERATED ALWAYS AS IDENTITY` ao lado do id real, e `JOIN` de volta na
busca **preservando a ordem do motor** (`ORDER BY s.score DESC`). O cliente nunca reordena: NDCG e MRR saem
do ranking do TheoDB, não do Python.

A chave é gerada pelo **banco**, não por contador no cliente, porque o runner de inserção concorrente copia
o cliente entre processos e dois contadores colidiriam. Atribuir chaves é trabalho do banco (degrau 3 da
parsimony ladder).

### R-2 — ALTO · `bm25_search` sobre índice nunca construído devolve zero linhas, sem erro

`SELECT count(*) FROM bm25_search(999,'lazy dog',5)` — onde 999 nunca passou por `bm25_build` — devolve
**0**, sem erro. **Indistinguível de "nada casou".**

Uma corrida que esquecesse o `optimize()` reportaria recall 0 como se fosse medição. O cliente consulta
`theodb.lexical_index_meta` antes de buscar e levanta — o catálogo responde como **fato**, em vez de o
cliente confiar numa flag própria que um processo copiado pode não carregar.

**O contorno é do cliente; a superfície pública segue exposta** — por isso virou [[B-041]] em vez de nota
de rodapé.

### R-3 — MÉDIO · Uma nota do próprio backlog que a medição derrubou

O [[B-004]] registrava, como achado lateral, que *"a superfície não expõe busca multi-termo"*. **Falso como
escrito**, e a nota teria feito este item parecer inviável no papel.

Medido: `bm25_search(1,'lazy dog',5)` devolve os dois documentos certos com scores acumulados (1,767 e 1,691
contra 0,883 e 0,845 de `dog` sozinho — OR com soma, que é BM25 correto), e a pergunta natural *"what does
the lazy dog do all day"* rankeia o documento certo em primeiro (5,543 contra 2,978).

O que de fato falta é mais estreito: operadores (`"frase"`, `AND`, `-exclusão`, `prefixo*`) e **stemming**.
Corrigido no B-004 por acréscimo.

### R-4 — BAIXO · O meu próprio critério de aceite reprovou o meu artefato

O T1.6 exigia que a primeira menção a `stemming` viesse antes da primeira a `NDCG`. Na primeira verificação:
**falhou** — `NDCG` aparecia na `description` do frontmatter (índice 176) antes de `stemming` (235).

Corrigi a descrição para liderar com o handicap, que é melhor redação: o cartão de resumo é o que o leitor
vê primeiro, e é onde a ressalva mais importa. O critério fez exatamente o trabalho para o qual foi escrito.

### R-5 — INFORMATIVO · O droplet recusou SSH depois de já ter conectado

Conectei, medi a spec, e a conexão seguinte deu `Connection refused` por ~7 minutos. `uptime` depois mostrou
`up 7 min`: o cloud-init reiniciou a máquina. Nada a consertar no produto — registrado porque um `until nc -z`
ingênuo teria girado sem diagnóstico, e o sinal certo (`uptime`) custou uma medição.

### R-6 — INFORMATIVO · O que este ciclo NÃO mediu, e é o que mais falta

**Nenhum outro motor foi rodado.** O leaderboard público tem Elasticsearch, OpenSearch e Milvus; comparar
citando os números publicados deles compararia máquinas, versões e datas diferentes — o erro que o B-035
documentou no eixo vetorial. O artefato diz isso explicitamente em vez de deixar o leitor supor.

## O que este review NÃO cobriu

- **Nenhum agente independente.** Mesmo agente que implementou.
- **Uma corrida, um dataset, um tamanho.** Sem variância medida, quanto mais significância pareada.
- **Sem comparação com outro motor** — ver R-6.
- **Os testes do cliente rodam no fork**, não neste repositório; `/code-quality` audita Rust.
- **O caminho concorrente do índice lexical** foi exercitado por dois threads no teste e por 80 clientes na
  corrida, mas sem injeção de falha (escrita durante leitura, VACUUM concorrente).
- **O CI segue vermelho** (B-029).

## Veredito

**`READY_TO_MERGE`.**

6 de 6 afirmações verificadas; nenhum gate duro disparou; 0 achados HARD. O `FAIL_SOFT` vem dos dois caps de
ambiente já diagnosticados no [[B-039]], num ciclo que não altera Rust.

**Ressalvas:** review do próprio implementador; o número publicado é de **uma** corrida sem teste de
significância; e o artefato não compara com nenhum outro motor — a comparação que o leaderboard convida
exige rodá-los na mesma máquina, e é trabalho seguinte.
