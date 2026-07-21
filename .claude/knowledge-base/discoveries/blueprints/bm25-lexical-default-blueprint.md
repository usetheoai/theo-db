---
slug: bm25-lexical-default
milestone_id: M138
date: 2026-07-21
---

# Blueprint — BM25 como perna lexical default

Fonte primária: build e execução reais do `pg_textsearch` v1.3.1 contra o nosso PostgreSQL **18.4**, mais o
artefato de medição `docs/benchmarks/m53-hybrid-beir.md` e os ADRs 0003/0013.

## Coverage Corner 1 — Integration tests

O `theodb_rs` já tem a perna BM25 implementada como **opt-in** (`lexical_engine='bm25'` em `hybrid.rs`), com o
template de fusão separado (`FUSION_TEMPLATE_BM25`) e 5 `pg_test` verdes segundo o M53. O que **nunca** foi
medido é a fusão **híbrida-com-BM25** contra a híbrida-com-`ts_rank_cd` — o M53 §4 registra isso como follow-up
explícito, porque a imagem daquela medição carregava o `theodb_rs` do M52, sem o parâmetro `lexical_engine`.

## Coverage Corner 2 — Dependencies

`timescale/pg_textsearch` v1.3.1 — **PostgreSQL License** (permissiva, D1-limpa, ADR 0003). GA de 2026-06-23,
Okapi BM25 verdadeiro (`k1=1.2`, `b=0.75`).

**MEDIDO nesta discovery (era o risco que podia matar o milestone):** o M53 construiu essa peça contra PG17, e o
M135 migrou tudo para PG18. Buildei v1.3.1 contra o PG18.4:

```
make PG_CONFIG=.../18.4/.../pg_config   → exit 0, ZERO erros, pg_textsearch.so produzido
make install                            → instalado em .../18.4/lib/postgresql/
CREATE EXTENSION pg_textsearch          → extversion 1.3.1
```

Sem patch, sem fork. **O M138 não está bloqueado pelo PG18.**

## Coverage Corner 3 — Tools

Nenhuma ferramenta nova. Build é PGXS puro (`make PG_CONFIG=...`), o que o encaixa direto no nosso
`Dockerfile` multi-stage — mesmo padrão do `Dockerfile.m53-bm25`, agora repontado para o 18.

## Coverage Corner 4 — Techniques

### T1 — Contrato operacional: exige `shared_preload_libraries` (medido)

`CREATE EXTENSION pg_textsearch` **falha** com erro tipado se a lib não estiver pré-carregada:

```
ERROR: pg_textsearch library not loaded. Add pg_textsearch to shared_preload_libraries and restart.
```

E a extensão valida a versão da lib carregada contra a do script SQL, falhando em drift. Consequência de
packaging: o `shared_preload_libraries` da imagem passa de `theodb_rs` para `theodb_rs,pg_textsearch`. É mudança
de configuração de servidor, não só de pacote — **quem atualizar precisa reiniciar**, e isso entra na nota de
migração.

### T2 — Superfície de API (medida, não presumida)

O índice exige `text_config` explícito no `WITH`, e a query usa `to_bm25query(texto, nome_do_indice)`:

```sql
CREATE INDEX bm_idx ON bm USING bm25 (body) WITH (text_config='english');
SELECT id, body <@> to_bm25query('lazy dog','bm_idx') AS score
FROM bm ORDER BY body <@> to_bm25query('lazy dog','bm_idx') LIMIT 3;
```

Note que o operador `<@>` **exige o `to_bm25query`** — `body <@> 'texto'` cru falha com
`no BM25 index found for text <@> text expression`. O nome do índice é argumento, o que acopla a query ao índice.

Ranking verificado como BM25 de verdade (normalização por tamanho de documento): sobre quatro docs, "the lazy dog
sleeps all day long" (curto) ranqueia **acima** de "the quick brown fox jumps over the lazy dog" (longo) para a
query `lazy dog`. Um `ts_rank_cd` não produziria essa ordem.

### T3 — O gap medido que justifica o milestone

`docs/benchmarks/m53-hybrid-beir.md` (BEIR scifact, 5.183 docs, 300 queries, 3 runs byte-idênticos):

| perna lexical | nDCG@10 | Recall@100 |
|---|---|---|
| `ts_rank_cd` — **shipado** | **0,0703** | 0,0694 |
| `pg_textsearch` BM25 | **0,6881** | 0,9182 |
| vetor (referência) | 0,7296 | 0,9733 |

Caveat herdado e mantido: o gap de ~9,8× **conflaciona ranker com candidate-set** — o `@@` do `ts_rank_cd`
derruba ~93% dos relevantes, enquanto o BM25 é top-k puro. O sinal limpo é BM25 0,688 ombro a ombro com o vetor
0,730 sobre o próprio top-k.

## ADRs

**ADR-1 — Adotar `pg_textsearch` como default, mantendo `ts_rank_cd` selecionável.** Alternativa rejeitada:
trocar sem escape — mudar o default altera resultados de queries existentes, e um usuário precisa de caminho de
volta. Alternativa rejeitada: esperar a engine própria (M140) — deixaria o usuário com 0,0703 por mais um
trimestre, e sem baseline contra o qual medir a engine própria.

**ADR-2 — O gate de adoção do ADR-0013 é executado, não reaberto.** O M53 declarou "o gate de medição está
executado"; este milestone consome essa decisão em vez de re-litigá-la. O que falta medir é a **fusão** com BM25,
não o leg isolado.

## Referências

- `docs/benchmarks/m53-hybrid-beir.md` (a medição decision-grade), `docs/adr/0003`, `docs/adr/0013`
- `theodb_rs/src/hybrid.rs` (o `lexical_engine` opt-in já implementado)
- `packaging/Dockerfile.m53-bm25` (o build PGXS, a repontar para o 18)
- build e execução reais contra PG18.4 nesta discovery (§ Corner 2, T1, T2)
