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

# Controle contra o pgvector real — a causa-raiz confirmada por implementação independente

Tudo acima é raciocínio sobre o nosso código contra a fonte do upstream. O controle que fecha o argumento é
rodar o **pgvector real** no cenário idêntico e ver se ele acerta onde nós erramos.

`ankane/pgvector:v0.5.1`, 20 000 × `vector(1536)`, mesmo `CREATE INDEX`, mesmo `ANALYZE`, mesma consulta:

| | páginas (heap / índice) | startup | plano escolhido |
|---|---|---|---|
| **pgvector** — *com* a correção | 128 / 20 001 | **324,60** | **`Index Scan using t_v_idx`** |
| **TheoDB** — *sem* a correção | 128 / 20 522 | 3 404,25 | `Seq Scan` |

As páginas batem (`heap = 128` idêntico), então os dois modelos veem o mesmo índice sobre a mesma tabela. A
única diferença de comportamento é a correção — e o startup difere **10,5×**, o que é exatamente o que decide
o plano.

**Isto confirma a causa-raiz por um caminho que não depende da minha leitura do código.**

## Correção da minha aritmética

A seção anterior previu que o startup corrigido seria **168,7**. O medido no pgvector é **324,60** — quase o
dobro. A previsão acertou a direção e a ordem de grandeza e **errou o número**, porque aplicou o nosso `ratio`
e o nosso `numIndexPages` a um `genericcostestimate` que no pgvector parte de outros valores.

Mantenho o cálculo registrado em vez de apagá-lo: ele foi apresentado com a ressalva de que era aritmética e
não execução, e essa ressalva era o que o tornava utilizável. O que ele provava — *a correção derruba o
startup para abaixo do custo do seqscan* — segue verdadeiro, agora por medição.

# O que NÃO foi testado

- **Outras dimensões.** Todo o experimento é 1536d. A correção escala com `numIndexPages`, então dimensões
  menores a atenuam — **onde ela deixa de bastar não foi medido**.
- **Escalas acima de 20k.**
- **Dimensões acima de 1536 e escalas acima de 50 000.**
- **Os três testes unitários não foram compilados nem executados.** Exigem PG18 configurado no pgrx (o
  `cargo pgrx init` está incompleto neste host) e a suíte não roda por B-001. O que
  prova a correção é a verificação ponta a ponta abaixo, não eles.

# Relacionados

- O runbook que trata o sintoma como erro do usuário: [diagnóstico do query vetorial](/runbooks/vector-scan-diagnostics.md)
- O dogfood que este defeito bloqueia: manifesto em `.claude/knowledge-base/dogfood/manifest.md`
- O perfil cujo tropeço isto explica: [SymQG com símbolos](/benchmarks/m184-symqg-profile-simbolos-verdict.md)

# A correção — aplicada e verificada ponta a ponta

`theodb_rs/src/am/cost.rs` ganhou `toast_startup_correction` como função pura (a suíte não executa
`#[pg_test]` — B-001 —, então tudo que dispensa uma `Relation` fica testável assim), e o `amcostestimate` em
`am/mod.rs` passou a chamá-la, lendo `spc_seq_page_cost` de `get_tablespace_page_costs` em vez de fixar
constantes — um tablespace em mídia diferente tem custos diferentes.

Imagem construída pelo Dockerfile do projeto, cenário-âncora reproduzido, **mesmas páginas** (`heap = 128`,
`idx = 20 522`):

| | startup | plano | tempo real |
|---|---|---|---|
| antes | 3 404,25 | `Seq Scan` | 182,117 ms |
| **depois** | **134,21** | **`Index Scan using t_ix`** | **6,401 ms** |

**28,5× mais rápido, com o planner escolhendo sozinho.**

## Varrimento — a correção vale no espectro, e as guardas funcionam

| dim | linhas | AM | heap / idx | usa índice | startup |
|---|---|---|---|---|---|
| 64 | 20 000 | hnsw | 741 / 1 237 | sim | 211,03 |
| 256 | 20 000 | hnsw | 2 858 / 3 380 | sim | 565,87 |
| 768 | 20 000 | hnsw | 128 / 10 522 | sim | 134,21 |
| 1 536 | **50 000** | hnsw | 319 / 51 302 | sim | 325,75 |
| 1 536 | 20 000 | **ivfflat** | 128 / 15 509 | sim | 143,00 |

A 64d e 256d o heap é grande — vetores desse tamanho não são TOASTed — e a guarda `startup_pages > rel_pages`
corretamente **não** dispara; o índice vence por já ser pequeno. A correção age exatamente onde o TOAST cria a
assimetria, que é o que o upstream desenhou.

## Um defeito do meu instrumento, quase publicado como cinco achados

A primeira leitura deste varrimento reportou `Seq Scan` nos cinco casos — contradizendo a verificação-âncora
que acabara de dar certo. O `grep` pegava a primeira ocorrência de `Seq Scan` na saída, e a subconsulta
`(SELECT v FROM t LIMIT 1)` do `InitPlan` sempre faz um, **antes** da linha do plano principal.

Cinco falsos negativos, coerentes entre si, prontos para serem lidos como "a correção não funciona em lugar
nenhum". O que os pegou foi a contradição com uma medição anterior — não uma revisão do script.

## Minhas duas previsões, e o quanto erraram

| | previsto | medido |
|---|---|---|
| startup corrigido (aritmética sobre nossas páginas) | 168,7 | — |
| startup do pgvector real | — | 324,60 |
| startup do nosso código corrigido | — | **134,21** |

A aritmética acertou a vizinhança e a direção nas duas vezes, e o número exato em nenhuma. Fica registrada
como o que era: um cálculo que orientou onde procurar, não uma medição.
