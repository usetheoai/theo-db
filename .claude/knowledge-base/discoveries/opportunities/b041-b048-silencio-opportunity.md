---
items: [B-041, B-048]
mode: bug
date: 2026-08-13
verdict: pending
---

# B-041 + B-048 — o pilar lexical responde onde deveria recusar, e o catálogo já sabe a diferença

## Corner 1 — Evidence

Medido em 2026-08-13 contra `theodb:b036`, contêiner limpo, tudo reproduzido por execução.

### B-041 — busca em índice nunca construído devolve silêncio

```
SELECT count(*) FROM bm25_search(999, 'lazy dog', 5);   -- 999 nunca passou por bm25_build
→ 0 linhas, sem erro, sem aviso
```

**Zero é indistinguível de "nada casou"**, e essa é a forma mais cara de falhar num pilar de busca: a
aplicação conclui que o corpus não tem o documento.

### B-048 (a) — `bm25_build` conta documento que nunca será achável

```sql
INSERT INTO docs VALUES (1,'the quick brown fox'), (2, NULL), (3,'lazy dog sleeps');
SELECT bm25_build(777, 'docs', 'id', 'body');   → 3
```

Mas só **dois** são acháveis:

```
'fox'                                    → 1 linha
'dog'                                    → 1 linha
'fox OR dog OR quick OR lazy OR brown OR sleeps'  → ids 1,3   (o 2 nunca aparece)
```

`engine.rs:157` faz `unwrap_or_default()` sobre o `body`: o `NULL` vira string vazia, entra no índice, conta
no retorno e não casa consulta nenhuma. **Quem confere o valor de retorno acredita que os 3 estão buscáveis.**

### B-048 (c) — `read_generation` mistura "sem build" com "a consulta falhou"

`engine.rs:105-110` encadeia `.ok()` → `.and_then()` → `.unwrap_or(0)`. Um erro do SPI ao consultar o catálogo
produz o mesmo `0` que um índice legitimamente não construído. **Não reproduzido por execução** — exigiria
fazer o SPI falhar de propósito —, e a distinção fica dita: este é achado de leitura.

### B-048 (b) — está FORA do binário default

`pg_backing.rs:201` (`Index::open` que falha devolve `0`) vive dentro de
`#[cfg(feature = "spike-lexical")]`, declarado no próprio arquivo como *"M186: andaime de medicao, fora do
default"*. **Não chega ao usuário.** O item o listou entre os três; a medição o retira do escopo de produto.

O caminho equivalente que **está** no default é `open_from_heap` (`engine.rs:115`), e ele já falha alto:
`error!("bm25: índice heap ilegível/corrompido para index_id={index_id}: {e}")`.

### O achado que decide o conserto: o catálogo distingue, e distingue bem

Esta é a medição que separa um conserto correto de um que quebraria o caso legítimo:

```sql
SELECT bm25_build(100,'cheio','id','body');   -- 1 documento  → 1
SELECT bm25_build(200,'vazio','id','body');   -- 0 documentos → 0

SELECT index_id, generation FROM theodb.lexical_index_meta ORDER BY index_id;
 index_id | generation
----------+-----------
      100 |         1
      200 |         1
```

**Um build com corpus vazio registra no catálogo com `generation 1`.** Então `lexical_index_meta` significa
*"um build aconteceu"*, e não *"existem documentos"* — que é exatamente a semântica de que o guard precisa:

| caso | catálogo | `bm25_search` deve |
|---|---|---|
| nunca construído | **ausente** | **erro tipado** nomeando o `index_id` |
| construído, corpus vazio | presente, `generation ≥ 1` | **zero linhas, sem erro** — resultado legítimo |
| construído, com documentos | presente | resultados |

**Correção do meu próprio raciocínio, registrada porque cheguei a formulá-la:** numa primeira leitura eu
concluí que o catálogo *não* distinguia os dois casos, e que o conserto exigiria mudar o `bm25_build`. Estava
errado — eu havia lido o catálogo **antes** de executar o segundo build. Re-medido em contêiner limpo, com os
dois builds na mesma transação, o registro está lá. **A ordem em que eu li produziu um defeito que não existe**,
e publicá-lo teria levado a uma mudança desnecessária no caminho de build.

### A classe, e por que ela é o item

O B-048 não é sobre três bugs — é sobre a **quarta a sexta reincidência** de uma classe que o projeto já
consertou três vezes: `explain_scan`/`scan_stats` devolvendo zeros silenciosos, o contador do chunk-skip do
colunar, e o gerador de script de upgrade. Somados ao [[B-034]] (GUC aceito sem efeito), ao [[B-041]] e ao erro
de parse engolido pelo `bm25_search` (consertado no [[B-044]]), são **seis instâncias documentadas**.

Uma classe que volta seis vezes não se resolve consertando a sétima.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `lexical/engine.rs` | `bm25_search` ganha o guard; `bm25_build` deixa de contar NULL; `read_generation` deixa de engolir erro |
| Consumidores | **o cliente do VectorDBBench já faz o guard por conta própria** (`theodb.py:330`) — é a evidência de que a informação está no lugar certo e a função no lugar errado. Ele pode simplificar depois, e não precisa |
| `theo-rag` / `theo-memory` | não usam o pilar lexical hoje (usam pgvector) — blast radius real de produto é zero neste momento |
| Corridas de benchmark publicadas | **nenhuma** muda: o b040/b044/b047 sempre construíram o índice antes de buscar |
| Compatibilidade | um erro novo onde antes havia silêncio **é** mudança de comportamento observável, e vai para o CHANGELOG como tal |

## Corner 4 — Verification

1. `bm25_search` sobre `index_id` ausente do catálogo levanta erro tipado nomeando o id — provado por teste que
   **hoje falharia**.
2. `bm25_search` sobre índice construído com corpus vazio continua devolvendo zero linhas **sem** erro — o teste
   distingue os dois, e é ele que impede o conserto de quebrar o caso legítimo.
3. `bm25_build` sobre tabela com `body NULL` devolve o número de documentos **acháveis**, e o teste compara o
   retorno com o que a busca de fato encontra — não com uma constante.
4. `read_generation` distingue "não construído" de "a consulta falhou".
5. O custo do guard é medido, não estimado: uma consulta a mais por busca é aceitável; um `JOIN` no caminho
   quente não é.

## Reclassificação

`suggested_mode: bug` mantido para os dois. O que a medição mudou: **(b) sai do escopo** (está atrás de feature
flag, fora do default) e **(c) é achado de leitura**, declarado como tal — só (a) e o B-041 foram reproduzidos
por execução.
