# Dogfood — manifesto

Registro do cenário-âncora que decide se este projeto pode alegar `production-ready` / v1.0.
Contrato: `.claude/rules/dogfood-golden-rule.md`. Avaliação: `/dogfood`.

## theo-rag-sobre-theodb

**Slug:** `theo-rag-sobre-theodb`

**Status:** `planned`

**Declarado em:** 2026-08-09 · **Proposto por:** claude-code/opus-5 · **Aguarda sign-off do owner**

### O cenário

O `theo-rag` — produto de RAG do próprio ecossistema, que serve usuários — usa o **TheoDB** como vector
store, em vez do pgvector, na infraestrutura que o time opera.

### Por que o status é `planned` e não outro

Aplicando o vocabulário do § 2 do golden rule contra o que existe no disco em 2026-08-09:

| status | exige | temos? |
|---|---|---|
| `planned` | âncora identificado, sem trabalho de implementação | **sim — é este** |
| `wired` | âncora invocado **ao menos uma vez** em CI ou smoke manual | não: o `theo-rag` nunca apontou para o TheoDB |
| `running` | **usado ativamente pelo time em infraestrutura real** | não |

**Medido, não presumido:** `theo-rag/package.json` declara `"compose:up": "docker compose up -d pgvector"`,
e o `theo-memory` declara o mesmo. Nenhum dos dois referencia o TheoDB.

### O que move este âncora adiante

**`planned` → `wired`:** o `theo-rag` sobe uma vez contra o TheoDB — compose apontando para a imagem
`ghcr.io/usetheodev/theo-db`, extensão criada, uma ingestão e uma consulta reais completando. Isso é
trabalho de engenharia e não depende de calendário.

**`wired` → `running`:** o time passa a **depender** disso — não uma execução, uso sustentado. É aqui que
mora a latência: os soft caps do § 4 pedem **≥ 3 evidências**, **≥ 1 failure story** e **≥ 2 operadores
distintos**, com a mais recente dentro de **30 dias**. Três evidências não se produzem numa tarde por
construção, e é por isso que declarar o âncora cedo importa: **a janela só começa a correr depois disto.**

### Progresso em 2026-08-09

**Drop-in verificado, executando contra a imagem** — a sequência exata que o `theo-rag` usa:

| passo | resultado |
|---|---|
| `CREATE EXTENSION IF NOT EXISTS vector` | OK — o shim reporta `vector 0.6.0`, **mais novo** que o `v0.5.1` que o theo-rag usava |
| `CREATE TABLE ... vector(1536)` | OK — o tipo que `packages/core/src/infrastructure/db/schema.ts` declara |
| INSERT de 500 vetores de 1536d | OK |
| `CREATE INDEX ... USING hnsw (v vector_cosine_ops)` | OK |
| `SELECT ... ORDER BY v <=> $1 LIMIT 10` | OK |

**PR aberto:** [usetheoai/theo-rag#206](https://github.com/usetheoai/theo-rag/pull/206) — troca a `image:`
do compose de dev. Uma linha; nome do serviço, variáveis e portas idênticos.

**O status continua `planned`, e não `wired`.** O § 2 define `wired` como *"implementation lands"* — o PR
está **aberto, não mergeado**. O smoke manual aconteceu, mas a implementação não aterrissou. Marcar `wired`
agora seria antecipar uma decisão que é da revisão do outro repositório.

**Não verificado, e declarado no próprio PR:** se o planner usa o índice em escala real (a 500 linhas ele
escolhe `Seq Scan`, o que é *correto* nesse tamanho e não diz nada sobre escala), e a suíte de testes do
`theo-rag` contra a imagem nova.

### Evidências

Nenhuma. `knowledge-base/dogfood/evidence/` está vazio, e o hard cap 3 (`no_anchor_evidence`) falha.

**Nenhuma evidência foi fabricada para preencher esta seção.** A regra lista *dogfood theatre* como o modo
de falha que ela existe para impedir, e um arquivo escrito por um agente que rodou o banco num container de
medição não é uso do time — é exatamente o teatro. As execuções desta sessão (medição de pilares, teste de
crash, perfil do SymQG) são **carga sintética de benchmark**, que o § 1 exclui em texto.

### Veredito atual

`EVIDENCE_INSUFFICIENT` — flag `anchor_missing` deixou de aplicar com este manifesto; o primeiro hard cap
a falhar agora é o **#2 (`anchor_not_running`)**, e em seguida o **#3 (`no_anchor_evidence`)**.

Enquanto isso valer, o `public-copy.md` § 3 proíbe `production-ready`, `production-grade` e
`battle-tested` — e **nenhum pilar passa de maturidade 4** (`wiki/benchmarks/m184-*`).
