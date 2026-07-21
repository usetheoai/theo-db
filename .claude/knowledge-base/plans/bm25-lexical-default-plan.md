---
slug: bm25-lexical-default
milestone_id: M138
created_at: 2026-07-21
goal: Trocar a perna lexical default de `ts_rank_cd` para BM25, gated por uma medição da FUSÃO que ainda não existe.
---

# Plano — M138: BM25 como perna lexical default

## Goal

Medir a fusão híbrida-com-BM25 contra a híbrida-com-`ts_rank_cd` em BEIR e, **se e somente se** a primeira for
superior com significância, promovê-la a default de `ai.hybrid_search_rrf`.

Métrica observável única: **nDCG@10 da fusão com BM25 > nDCG@10 da fusão com `ts_rank_cd`**, com teste pareado
sobre as 300 queries (p < 0,05).

## Context

A perna lexical **shipada** mede nDCG@10 **0,0703**; o BM25 do `pg_textsearch` mede **0,6881** (M53, BEIR scifact,
5.183 docs, 300 queries). Mas o M53 mediu o **leg isolado**, não a fusão — e a própria fusão com `ts_rank_cd` já
empatava com o vetor puro (0,7337 vs 0,7296) porque a perna lexical contribuía pouco para o RRF. **Não está
provado que trocar a perna melhora o produto.** Essa medição é o coração deste milestone.

Consome `discoveries/blueprints/bm25-lexical-default-blueprint.md`, cuja discovery já eliminou o risco
bloqueador: `pg_textsearch` v1.3.1 compila e opera no PG18.4 sem patch.

## Baseline Context

### Files that will be touched

| Arquivo | LoC (medido) | Papel |
|---|---|---|
| `theodb_rs/src/hybrid.rs` | 460 | fusão RRF; já tem `lexical_engine` opt-in e `FUSION_TEMPLATE_BM25` |
| `theodb_rs/src/api.rs` | 957 | superfície SQL de `ai.hybrid_search_rrf` |
| `Dockerfile` | 100 | build do `pg_textsearch` + `shared_preload_libraries` |
| `theodb_rs/sql/theodb_rs--1.1.0--1.2.0.sql` | — | **NEW** — delta (a cadeia do M137 já existe) |
| `benchmarks/run_m53_hybrid_beir.py` | — | harness reusável da medição |
| `docs/benchmarks/m138-bm25-fusion.md` | — | **NEW** — o artefato |

### Current callers / dependents

- `ai.hybrid_search_rrf` é a superfície pública; `hybrid.rs:153` seleciona o template pelo `lexical_engine`.
- O leg BM25 exige `content_text_col` (a coluna TEXT indexada `USING bm25`), não a coluna `tsvector` do default.
- `docs/benchmarks/m53-hybrid-beir.md` é o baseline contra o qual esta medição compara.

### Domain glossary

- **RRF** — Reciprocal Rank Fusion: funde rankings por `1/(k + rank)`, não por score bruto. Por isso um leg fraco
  contribui pouco, e é exatamente por isso que a superioridade do leg **não implica** superioridade da fusão.
- **Leg isolado vs fusão** — o M53 mediu o primeiro; este milestone mede o segundo.
- **`to_bm25query(texto, indice)`** — a API do `pg_textsearch`; o operador `<@>` não aceita texto cru, e o nome do
  índice é argumento, acoplando query a índice.

### Architecture boundaries affected

**Nenhuma.** A troca acontece dentro de `hybrid.rs`, que já é o ponto de seleção. O `pg_textsearch` entra como
dependência de distribuição (imagem), não como acoplamento de código — `hybrid.rs` fala SQL com ele.

## Prior Art & Related Work

- **Blueprint** `discoveries/blueprints/bm25-lexical-default-blueprint.md` (T1–T3, ADR-1..2).
- **`docs/benchmarks/m53-hybrid-beir.md`** — a medição decision-grade do leg e o follow-up §4 que este plano executa.
- **ADR 0003** (identificação do `pg_textsearch`, D1-limpo) e **ADR 0013** (exceção permissiva gated).
- **M123/M125** — precedente nosso de teste de significância pareado sobre BEIR (permutação pareada numpy).

## ADRs

### ADR-1 — A troca de default é GATED pela medição da fusão, não pelo leg

**Decisão:** só promover BM25 a default se a fusão com ele bater a fusão com `ts_rank_cd` com significância.
**Alternativa rejeitada:** trocar com base no leg isolado (0,688 vs 0,070) — é o erro que o RRF pune: fusão não
herda a superioridade do componente. O M53 já mostrou fusão ≈ vetor mesmo com leg fraco.
**Consequência aceita:** este milestone pode terminar em **honest-negative** — leg adotado como opt-in melhorado,
default mantido — e isso é resultado válido, não fracasso.

### ADR-2 — `ts_rank_cd` permanece selecionável

**Decisão:** trocar o default sem remover a opção.
**Alternativa rejeitada:** troca dura — muda resultado de query existente sem caminho de volta.

## Dependencies

| Dep | Versão | Já instalada? | Regra 9 |
|---|---|---|---|
| `pg_textsearch` | v1.3.1 | build validado no PG18.4 nesta discovery | PostgreSQL License, D1-limpa (ADR 0003); nenhuma peça own-code permissiva resolve hoje |

## Phase 1 — A medição que decide

### T1.1 — Fusão com BM25 vs fusão com `ts_rank_cd` em BEIR

#### Why this step

É o follow-up que o M53 §4 registrou e nunca rodou, e é o que decide o milestone inteiro. A fusão RRF combina por
rank, não por score, então um leg 9,8× melhor **pode** não mover a fusão — e o M53 já mostrou a fusão empatando
com o vetor puro. Medir antes de trocar é o que separa este projeto de um que troca por intuição.

#### TDD

```
RED: test_m138_fusion_bm25_vs_tsrank
     Given BEIR scifact (5.183 docs, 300 queries, qrels binário) no PG18.4
     When  ai.hybrid_search_rrf roda com lexical_engine='bm25' e com 'ts_rank_cd'
     Then  os nDCG@10 das DUAS fusões são produzidos, com teste pareado por query
     (hoje: só o leg isolado foi medido; a fusão com BM25 nunca rodou)
```

#### Files to edit
- `benchmarks/run_m138_bm25_fusion.py` (NEW — reusa `theodb_bench.{beir,hybrid,metrics,db}`)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `python benchmarks/run_m138_bm25_fusion.py` retorna exit code 0 e escreve nDCG@10 e Recall@100 das **duas** fusões sobre o MESMO corpus/queries/embeddings.
- O teste **pareado** sobre as 300 queries emite um p-valor `< 0.05` OU registra explicitamente `p >= 0.05` (honest-negative), verificável no stdout `paired p = <valor>`.
- O artefato contém a linha `vetor puro nDCG@10 = <valor>` como referência, para situar as duas fusões.

#### DoD
- `docs/benchmarks/m138-bm25-fusion.md` publicado com os quatro números e o p-valor.

## Phase 2 — A troca (gated pela Phase 1)

### T2.1 — Promover BM25 a default, se e somente se a medição autorizar

#### Why this step

Executa o ADR-1. Se a fusão com BM25 vencer com significância, o default muda; se não, o milestone reporta
honest-negative e o default permanece — com o leg opt-in melhorado e documentado.

#### TDD

```
RED: test_m138_default_lexical_engine_is_bm25
     Given ai.hybrid_search_rrf chamado SEM lexical_engine explícito
     When  o template de fusão é selecionado
     Then  é o de BM25   (só se a Phase 1 autorizar; senão este teste não existe)
```

#### Files to edit
- `theodb_rs/src/hybrid.rs` (o default em `:153`)
- `theodb_rs/src/api.rs` (a doc da superfície)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `cargo test test_m138_default_lexical_engine_is_bm25` retorna exit code 0: a chamada sem `lexical_engine` seleciona o template BM25 (`assert_eq!` sobre o SQL gerado).
- `cargo test test_m138_tsrank_still_selectable` retorna exit code 0: `lexical_engine='ts_rank_cd'` continua produzindo o template legado (o escape do ADR-2).

#### DoD
- Ambos os caminhos verdes no PG18.4.

### T2.2 — `pg_textsearch` na distribuição + cadeia de upgrade

#### Why this step

O leg default não pode depender de uma extensão ausente da imagem. E a discovery mediu que ele exige
`shared_preload_libraries`, o que significa **reinício na atualização** — isso precisa estar na nota de migração,
não ser descoberto pelo usuário.

#### Files to edit
- `Dockerfile` (build PGXS do `pg_textsearch` + `shared_preload_libraries=theodb_rs,pg_textsearch`)
- `theodb_rs/sql/theodb_rs--1.1.0--1.2.0.sql` (NEW — delta, per ADR-1 do M137)
- `theodb_rs/theodb_rs.control` (`default_version` → `1.2.0`)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `docker build` retorna exit code 0 e, no container, `SELECT extversion FROM pg_extension WHERE extname='pg_textsearch'` retorna `1.3.1`.
- `bash scripts/test-upgrade.sh` retorna exit code 0 com `TO_VER=1.2.0` na cadeia (o harness do M137, que aborta em pass vacuoso).
- A nota de migração em `docs/benchmarks/m138-bm25-fusion.md` contém a string `exige reinício` descrevendo o `shared_preload_libraries`.

#### DoD
- Imagem builda e o harness de upgrade passa.

## Failure scenarios

| Cenário | Como o teste reproduz | Comportamento esperado |
|---|---|---|
| `pg_textsearch` ausente + `lexical_engine='bm25'` | chamar sem a extensão instalada | erro tipado `0A000`, não crash (comportamento já existente) |
| Lib não pré-carregada | `CREATE EXTENSION` sem `shared_preload_libraries` | erro tipado nomeando a causa (medido na discovery) |
| Drift de versão da lib vs SQL | binário novo, script velho | a própria extensão falha com mensagem de versão (medido) |

## Coverage Matrix

| Afirmação do Goal | Tarefa(s) |
|---|---|
| medir a fusão com BM25 vs com ts_rank_cd | T1.1 |
| teste pareado com significância | T1.1 |
| promover a default se e somente se autorizado | T2.1 |
| `ts_rank_cd` continua selecionável | T2.1 |
| dependência na distribuição + upgrade | T2.2 |

100% — nenhuma afirmação sem tarefa.

## Drawbacks & Risks

| # | Risco | Severidade | Mitigação | Dono |
|---|---|---|---|---|
| R1 | A fusão com BM25 **não** bater a com `ts_rank_cd` — o RRF funde por rank e pode lavar a diferença | ALTA | é o ponto do ADR-1: honest-negative é resultado válido, e o milestone reporta isso em vez de trocar assim mesmo | impl |
| R2 | `shared_preload_libraries` muda ⇒ atualização exige **reinício de servidor**, não só de pacote | MÉDIA | nota de migração explícita é item de DoD da T2.2 | impl |
| R3 | `pg_textsearch` passa de exceção gated a dependência embarcada, e o M140 pode substituí-la | MÉDIA | churn aceito conscientemente (registrado no milestone); a alternativa é deixar o usuário em 0,0703 por mais um trimestre | owner |
| R4 | A API acopla query a índice (`to_bm25query(texto, indice)`), então o nome do índice vaza para o SQL gerado | MÉDIA | `hybrid.rs` já resolve o nome do índice; cobrir com teste de que o quoting é `%I` (injeção) | impl |

## Unresolved Questions

- Q1 — **O corpus scifact favorece o vetor** (claims científicos, paráfrase), o que é justamente onde o lexical
  sofre. Se a fusão com BM25 não vencer aqui, isso não prova que não vence num corpus lexical-heavy — o M125 já
  usou NFCorpus por essa razão. Medir os dois seria mais honesto, e fica como decisão da Phase 1.
- Q2 — **Não sabemos o custo de build/tamanho do índice BM25** sobre o corpus real. Se for proibitivo, o default
  muda de conversa. A medição da Phase 1 deve registrar tempo de build e tamanho, não só qualidade.
- Q3 — **`pg_textsearch` não tem cadeia de upgrade nossa** — é dependência externa. Se ela mudar de versão, o
  `shared_preload_libraries` e o script SQL precisam casar. Fora do escopo deste milestone, registrado.

## Global DoD

- [x] Artefato com nDCG@10 e Recall@100 das duas fusões + vetor de referência, sobre o mesmo corpus (`docs/benchmarks/m138-bm25-fusion.md`, scifact **e** NFCorpus).
- [x] Teste pareado sobre as queries com p-valor reportado (scifact p=0,51; NFCorpus p=0,0168).
- [x] Default trocado **se e somente se** a medição autorizar; caso contrário, honest-negative documentado — **a medição NÃO autorizou; honest-negative documentado**.
- [x] `lexical_engine='ts_rank_cd'` continua funcionando (permanece o default, inalterado).
- [x] CHANGELOG `[Unreleased]` atualizado (Regra 6).
- [n/a] Imagem builda com `pg_textsearch` + preload / cadeia de upgrade / nota de reinício — **não executado** (Phase 2 gated pela Phase 1; a medição vetou a troca, então embarcar `pg_textsearch` seria complexidade sem ganho medido — ver Outcome).

## Outcome (medido 2026-07-21 — HONEST-NEGATIVE)

A Phase 1 (a medição-que-decide) rodou em DOIS corpora BEIR na droplet PG18.4 e **vetou a troca de default**:

| corpus | fusão ts_rank_cd | fusão BM25 | mean_diff | p (pareado) | decisão |
|---|---|---|---|---|---|
| scifact | 0,7337 | 0,7418 | +0,0081 | **0,51** | não troca (empate) |
| NFCorpus | 0,3946 | 0,3797 | −0,0149 | **0,0168** | não troca (**BM25 pior, significativo**) |

Cross-check: `hybrid_tsrank` (twin) = 0,733724 = o in-DB do M123 → o harness é fiel, os números são reais.

Por ADR-1, honest-negative é resultado válido. A **Phase 2 não foi executada** (correto — gated pela Phase 1):
o default permanece `ts_rank_cd`; `pg_textsearch` **não** é embarcado. Achado colateral filado: **issue #146**
(fusão in-DB `lexical_engine='bm25'` quebrada no pg_textsearch 1.3.1 — nunca exercida antes). O valor entregue
é a medição decision-grade que o M53 registrou e nunca rodou, mais o bug que só apareceu ao exercer a perna
ponta-a-ponta.
