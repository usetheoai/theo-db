---
slug: b044-stemming
items: [B-044]
date: 2026-08-13
base: 96fb342
head: dd5254d
verdict: READY_TO_MERGE
---

# Review — o stemming entrou, e o controle na mesma máquina desfez uma conclusão minha

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Suíte completa | **469 passed, 0 failed** (era 457; +12 deste ciclo) |
| 2 | Segredos commitados | **0** |
| 3 | Commit direto em `main` | não — `workspace` |
| 4 | Trailer de coautoria | **0** |
| 5 | `CHANGELOG.md` atualizado | sim — Added, Changed e Fixed |
| 6 | Bundle OKF | **303 conceitos, 0 erros, 0 warnings, 0 órfãos** |

`/code-quality`: **`FAIL_SOFT`**, Rust auditado, **0 achados HARD**, nada novo no módulo lexical. Os dois
caps são os de ambiente — e desta vez rodei o `cargo-udeps` **contra este código** dentro do contêiner
pinado: **`All deps seem to have been used.`**

## Cross-validation — 4 de 4

| # | Afirmação do Goal | Como foi verificada | Resultado |
|---|---|---|---|
| G1 | `jumping` casa `jumps`; stopwords removidas | 6 unitários do analisador + `pg_test_bm25_matches_across_inflection`, `..._stems_both_sides_of_the_pipeline`, `..._stopword_only_query_returns_no_rows_without_error` | ok |
| G2 | Índice antigo responde como antes, sem migração | `pg_test_legacy_default_schema_index_keeps_its_own_semantics` — constrói um índice com schema legado e prova que ele **não** stemiza; `..._rebuilding_a_legacy_index_upgrades_it` prova o caminho de atualização | ok |
| G3 | Tokenizer desconhecido falha alto | `engine.rs:216` — `Err(e) => error!(...)`; `pg_test_sanitized_user_queries_never_reach_the_error_path` prova que consulta de usuário não cai lá | ok |
| G4 | Efeito medido no mesmo caso do `b040` e publicado | A/B controlado no droplet; artefato **atualizado**, não duplicado | ok |

Verificado por nome, não pela contagem: os 12 testes aparecem individualmente na saída filtrada.

## O resultado

A/B controlado — **mesma máquina** (`g-16vcpu-64gb`, Xeon 8358, IP 159.65.249.69, destruído), mesmo caso,
mesmo dataset em cache, **só a imagem muda**:

| | NDCG@10 | recall@10 | MRR | QPS | p99 serial | build |
|---|---|---|---|---|---|---|
| sem stemming (`b034`) | 0,6962 | 0,8025 | 0,6670 | 1.722,6 | 3,9 ms | 2,19 s |
| **com stemming (`b044`)** | **0,7351** | **0,8464** | **0,7034** | **1.910,3** | **3,5 ms** | 3,21 s |
| delta | **+5,6%** | **+5,5%** | **+5,5%** | **+10,9%** | **−10,3%** | +46,9% |

Qualidade sobe nos três eixos **e o throughput sobe junto**. Remover stopwords encurta as listas de postings
mais do que o stemmer as alonga. O único custo é o build.

## Achados

### R-1 — ALTO · Publiquei −31,8% de QPS, e estava errado por confound de hardware

A primeira corrida com stemming rodou num droplet **Xeon 8168**; a corrida sem, feita no ciclo anterior,
num **Xeon 8358**. O delta aparente era **−31,8% de QPS**, e eu o teria atribuído ao stemmer.

Refeito na mesma máquina: **+10,9%**. O sinal inverteu.

NDCG, recall e MRR **não mudaram** entre as duas leituras — são função determinística do índice e das
consultas. QPS e p99 mudaram inteiramente. A distinção é a lição: **métricas de qualidade são independentes
de hardware; as de velocidade não são.**

É o erro do `b035` num eixo novo. Lá o parâmetro era igual (`ef_search=64` dos dois lados) e o ponto de
operação não. Aqui o rótulo era igual (mesma corrida, mesmo caso) e a **máquina** não.

**O que tornou o conserto possível foi ter refeito antes de publicar.** O ADR-0061 já exigia mesma máquina
para concorrentes; foi estendido para cobrir antes-e-depois do mesmo motor.

### R-2 — MÉDIO · A corrida controlada carrega uma sonda que prova qual variante está ativa

Não confiei no rótulo da imagem. Cada corrida do A/B roda antes um `bm25_build` de 1 documento e consulta
`jumping`, imprimindo `stemming ativo: 0` ou `1`. A saída mostra `0` na primeira corrida e `1` na segunda.

Sem isso, uma troca de imagem que silenciosamente falhasse produziria duas corridas idênticas rotuladas como
A e B — e um delta de zero seria lido como "o stemming não faz diferença".

### R-3 — MÉDIO · O desenho eliminou a migração em vez de tratá-la

O plano previa "a invalidação de índices existentes está tratada". A descoberta mostrou que **não há o que
tratar**: o Tantivy serializa o nome do tokenizer no schema de cada índice, então um índice antigo diz
`"default"` para sempre e continua resolvendo o tokenizer padrão.

Registrar sob nome próprio (`theodb_en`) em vez de redefinir `"default"` transforma um problema de migração
em não-problema. **Redefinir `"default"` teria mudado a semântica de busca de toda instalação existente em
silêncio** — consulta stemizada contra índice não stemizado.

Não escrevi script de rebuild, e o `git diff` prova. O que escrevi foi o **teste** que verifica a afirmação:
constrói um índice com schema legado e assere que ele não stemiza.

### R-4 — MÉDIO · Um erro engolido a menos

`engine.rs:188` devolvia lista vazia em erro de parse. Um `UnknownTokenizer` — a falha exata que um registro
malfeito produz — passaria como "nada casou", e uma corrida de benchmark publicaria NDCG 0 como medição.

Agora falha alto. O risco de virar erro para consulta de usuário está coberto: `sanitize_query` reduz a
consulta a alfanuméricos antes do parser, e um teste parametrizado sobre `"LAZY, Dog!"`, `"a+b"`,
`"x AND -y"`, `"acentuação"`, `"((("` e `"\"aspas\""` prova que nenhuma delas chega lá.

### R-5 — BAIXO · Ordem dos filtros, decidida por raciocínio e não por acaso

`StopWordFilter` vem **antes** do `Stemmer`. Depois dele, `the` já teria virado outro radical e não casaria
a lista de stopwords. Está no comentário do código porque é o tipo de detalhe que um refactor futuro
inverteria sem perceber.

### R-6 — INFORMATIVO · O cap do `cargo-udeps` tem contra-evidência, de novo

Este ciclo **altera Rust**, então o cap importa mais que nos dois anteriores. Rodado no contêiner pinado
contra este código: **`All deps seem to have been used.`** O detector do `/code-quality` continua invocando
no host, onde o pgrx não existe — é o [[B-039]].

## O que este review NÃO cobriu

- **Nenhum agente independente.** Mesmo agente que implementou.
- **Sem significância pareada.** Duas corridas, uma de cada lado. O delta de +5,6% em NDCG é **observado**,
  não demonstrado — [[B-045]].
- **Um corpus, um tamanho, um idioma.** MS MARCO 100K em inglês. Nada diz sobre outro idioma, e a D2 fixou
  inglês deliberadamente.
- **Stemming pode ter piorado consultas específicas** — o NDCG agregado subiu, mas não inspecionei consultas
  individuais. Porter é agressivo (`university`/`universe` colidem) e o efeito por consulta não foi medido.
- **Operadores de consulta continuam ausentes** — fora do escopo por decisão.
- **O CI segue vermelho** (B-029).

## Veredito

**`READY_TO_MERGE`.**

4 de 4 afirmações verificadas por teste nomeado; 469 testes verdes; 0 achados HARD; o efeito medido em A/B
controlado e publicado com a correção da minha própria leitura anterior.

**Ressalvas:** review do próprio implementador; o ganho é observado sem teste de significância; e o número
que este ciclo mais ensina não é o +5,6% de NDCG — é que **a mesma medição, em máquinas diferentes, deu
−31,8% e +10,9% de QPS**.
