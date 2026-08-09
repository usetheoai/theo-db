---
type: Measurement
title: m175 — o planner escolhe um plano 91× mais lento porque o custo do índice HNSW está superestimado em 94×
description: A 20 mil linhas o índice responde em 2 ms e o seq scan em 182 ms, mas o modelo de custo estima o índice como 94× mais caro — então ele nunca é escolhido sem intervenção manual.
resource: benchmarks/artifacts/m175/planner-cost-inversion.json
tags: [benchmark, m175, planner, cost-model, hnsw, defeito, dogfood, bloqueia-migracao]
milestone: M175
generated: { by: claude-code/opus-5, at: 2026-08-09T12:00:00Z }
sources:
  - id: inv
    resource: benchmarks/artifacts/m175/planner-cost-inversion.json
    title: EXPLAIN ANALYZE dos dois planos, 20k linhas, vector(1536)
---

Achado ao verificar o drop-in do [dogfood](/benchmarks/m184-pilares-superficie-medida-verdict.md) — o
`theo-rag` migrando do pgvector para o TheoDB. **Não era o que se procurava, e é mais grave que o que se
procurava.**

# A medição

20 000 linhas, `vector(1536)`, índice criado por sintaxe pgvector
(`USING hnsw (vector vector_cosine_ops)`), `ANALYZE` rodado:

| plano | custo estimado | **tempo real** |
|---|---|---|
| default — `Sort` + `Seq Scan` | 830,19..880,19 | **182,117 ms** |
| forçado — `Index Scan using chunks_vector_idx` | **3 404,25..83 080,00** | **1,994 ms** |

**O índice é 91× mais rápido e é estimado como 94× mais caro.** O modelo de custo está invertido, e o
planner escolhe sistematicamente o plano pior.

# Por que isto importa mais que um número de benchmark

**Todo usuário que criar um índice vetorial recebe um índice que nunca é usado** — a menos que saiba
executar `SET enable_seqscan=off; SET enable_sort=off`, que não está em nenhum caminho documentado de
uso normal.

O [runbook de diagnóstico](/runbooks/vector-scan-diagnostics.md) abre dizendo exatamente isto:

> **A causa nº 1 de "recall ou latência ruim" NÃO é o `ef` — é o planner não escolher o índice.**

O runbook trata isso como erro de configuração do usuário. **A medição mostra que é o comportamento
default do produto** em escala onde o índice é decisivamente melhor.

**Bloqueia o dogfood.** O PR [usetheoai/theo-rag#206](https://github.com/usetheoai/theo-rag/pull/206)
migra o `theo-rag` para o TheoDB, e o drop-in funciona — mas o índice que ele criaria não seria usado. A
migração entregaria buscas 91× mais lentas que o esperado, sem erro nenhum que denunciasse.

**Explica um tropeço anterior.** Duas tentativas de perfilar a busca do SymQG não produziram amostra
([m184](/benchmarks/m184-symqg-profile-simbolos-verdict.md)) porque o planner não usava o índice — eu
tratei como detalhe de bancada. Era este defeito.

# Causa-raiz — identificada no código, contra a fonte primária

O acervo local não está no disco, então o `hnswcostestimate` do pgvector foi obtido da fonte primária na web
([`pgvector/src/hnsw.c`, master](https://raw.githubusercontent.com/pgvector/pgvector/master/src/hnsw.c),
lido em 2026-08-09).

**O ratio não é o defeito.** `am/cost.rs:33-42` porta a matemática do pgvector fielmente — `entryLevel`,
`layer0TuplesMax`, `layer0Selectivity`, `scalingFactor = 0.55` — e produz ~4,1% a 20k, coerente com o
`3404,25 / 83080,00 = 0,04098` medido. O núcleo `startup = total * ratio` também é idêntico ao do upstream.

**O defeito é uma correção que omitimos deliberadamente.** Logo após o núcleo, o pgvector aplica:

```c
/* Adjust cost if needed since TOAST not included in seq scan cost */
startupPages = costs.numIndexPages * ratio;
if (startupPages > path->indexinfo->rel->pages && ratio < 0.5)
{
    costs.indexStartupCost -= startupPages * (costs.spc_random_page_cost - spc_seq_page_cost);
    costs.indexStartupCost -= (startupPages - path->indexinfo->rel->pages) * spc_seq_page_cost;
}
```

`am/mod.rs:208-212` para no núcleo e nunca aplica isso. A omissão é **documentada e justificada** em
`am/cost.rs:10-14`:

> as correções secundárias […] "only shift cost slightly *toward* the index and **never flip the
> index-vs-seqscan choice this feature exists to make honest**. Omitting them biases conservatively toward
> seqscan."

**A medição refuta essa justificativa.** A correção omitida existe precisamente para o caso de vetores
grandes — seu próprio comentário no upstream diz *"TOAST not included in seq scan cost"*, e um `vector(1536)`
são 6 KB por linha, sempre TOASTed. É o regime em que a correção é máxima, não desprezível. As duas
pré-condições do `if` estão satisfeitas no cenário medido (`ratio = 0,041 < 0,5`).

Chamar de "enviesar conservadoramente para o seqscan" descreve corretamente o efeito e **subestima sua
magnitude**: o viés não é conservador, é decisivo — ele nunca deixa o índice ser escolhido.

## O que isso ensina, além do defeito

A omissão foi **raciocinada, escrita e revisada** — não foi um esquecimento. O que faltou foi a medição que
testasse a frase "never flip the choice", que é uma afirmação empírica apresentada como evidente. É a mesma
classe que este projeto já registrou várias vezes: **uma hipótese plausível sobre performance, escrita com
confiança, que só uma medição derruba.**

## Quantificação — a correção omitida é o que decide

Páginas medidas no cenário real: `idx_pages = 20 522`, `heap_pages = 128` (pequeno **porque os vetores estão
TOASTed** — exatamente o que o comentário do upstream descreve), `ratio = 0,040975`.

As duas pré-condições do `if` estão satisfeitas: `startupPages = 840,9 > 128` e `ratio = 0,041 < 0,5`.

| | valor |
|---|---|
| `startupPages * (random − seq)` = 840,9 × 3 | −2 522,7 |
| `(startupPages − heap_pages) * seq` = 712,9 × 1 | −712,9 |
| **startup nosso** | 3 404,25 |
| **startup com a correção** | **168,7** — queda de **95%** |
| custo do plano `Seq Scan` | 810,21 |

**168,7 < 810,21 — com a correção o índice vence.** A omissão não é "um ajuste leve que nunca inverte a
escolha": ela é *a* coisa que inverte a escolha.

# Hipótese minha, refutada por medição

Antes de ler o `cost.rs` eu propus uma explicação concorrente: as funções de distância declaram `procost = 1`
(verificado em `pg_proc`), então o planner cobraria uma comparação de 1536 dimensões como uma única operação
de CPU, subestimando o `Sort`.

**Testada e falsa.** `ALTER FUNCTION theodb_vector_cosine_distance COST` em 1, 10, 100 e 1000 — `Seq Scan`
nos quatro. O custo do operador não é o mecanismo. Registro porque a hipótese era plausível e teria mandado
a correção para o lugar errado.

# Escopo medido do defeito

| eixo | resultado |
|---|---|
| **Cruzamento** 1k / 5k / 20k | `Seq Scan` nos três — **não há cruzamento nessa faixa**. Não é questão de escala. |
| **`theodb_ivfflat`** a 20k | `Seq Scan` também. **Mesmo defeito** — o `cost.rs` omite a correção `sequentialRatio` equivalente pelo mesmo argumento. |

**Achado lateral:** o shim de compatibilidade pgvector não expõe `vector_cosine_ops` para o `theodb_ivfflat`
(`ERROR: operator class "vector_cosine_ops" does not exist for access method "theodb_ivfflat"`) — só para o
`hnsw`. Quem migrar do pgvector e quiser IVF precisa reescrever o `CREATE INDEX`.

# O que NÃO foi testado

- **Outras dimensões.** Todo o experimento é 1536d. A correção escala com `numIndexPages`, então dimensões
  menores a atenuam — **onde ela deixa de bastar não foi medido**.
- **Escalas acima de 20k.**
- **A correção aplicada de fato.** Os 168,7 são **aritmética sobre páginas medidas**, não um plano executado —
  confirmar exige recompilar a extensão. É a primeira coisa que o M185 deve fazer, e ela pode desmentir o
  cálculo.

# Relacionados

- O runbook que trata o sintoma como erro do usuário: [diagnóstico do query vetorial](/runbooks/vector-scan-diagnostics.md)
- O dogfood que este defeito bloqueia: manifesto em `.claude/knowledge-base/dogfood/manifest.md`
- O perfil cujo tropeço isto explica: [SymQG com símbolos](/benchmarks/m184-symqg-profile-simbolos-verdict.md)
