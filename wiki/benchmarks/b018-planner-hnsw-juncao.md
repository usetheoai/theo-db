---
type: Measurement
title: b018 — o planner larga o HNSW na junção filtrada, e a causa é o TAMANHO do índice, não o modelo de custo
description: Reproduzido deterministicamente. A causa é o DEFAULT de ef_search — 64 nosso contra 40 do pgvector. No mesmo ef, o pgvector produz plano e custos IDÊNTICOS aos nossos, e nosso índice é menor. Preserva duas conclusões minhas anteriores que a medição derrubou, uma delas por dados degenerados.
tags: [planner, hnsw, custo, juncao, b-018, honest-negative]
item: B-018
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

# O achado, medido sobre dados válidos

> Esta seção **substitui** duas conclusões anteriores deste mesmo arquivo. As duas ficam preservadas
> ao final, sob § Retratações — apagá-las esconderia que foram citadas no `BACKLOG.md`.

## O que reproduz

Um **filtro seletivo na tabela juntada** é o gatilho que os seis cenários de 2026-08-11 não tocaram:

```sql
SELECT e.id FROM embeddings e
  JOIN chunks c ON c.id = e.chunk_id
  JOIN documents d ON d.id = c.document_id
 WHERE d.tenant = 't1'                       -- <<< sem isto, o plano é o correto
 ORDER BY e.vector <=> $1 LIMIT 5;
```

Com o filtro, a ordem de junção inverte, `embeddings` vai para o lado interno de um Nested Loop por
`chunk_id` e um `Sort` aparece — a forma exata do relato (`Limit → Sort → Nested Loop → Index Scan`).
Com `enable_sort = off` o plano não muda, apenas ganha `Disabled: true`: não há caminho alternativo a
gerar.

## A causa: o DEFAULT de `ef_search`, e nada mais

3000 vetores **distintos** de 384 dimensões, esquema `documents`/`chunks`/`embeddings` idêntico nos
dois motores:

| motor | `ef_search` | custo do `Limit` | plano |
|---|---|---|---|
| TheoDB | 40 | 434,34 | **HNSW** |
| TheoDB | **64 (nosso default)** | **567,66** | **Sort** |
| pgvector 0.8.6 | 40 (default dele) | 478,41 | **HNSW** |
| pgvector 0.8.6 | **64** | **567,66** | **Sort** |

**Em `ef_search = 64` o pgvector produz o plano e os custos IDÊNTICOS aos nossos** — 567,66 / 559,36 /
560,45 / 552,13, número por número. Não é aproximação: é o mesmo modelo, e o `am/cost.rs` é port fiel
do `hnsw.c` do pgvector 0.8 (mesmo `ratio`, mesmo `scalingFactor = 0,55`, mesma correção TOAST).

**Isto não é defeito de implementação nosso.** No mesmo `ef`, nosso index scan custa **425,60** contra
**469,68** dele, e nosso índice ocupa **680 páginas** contra **751** — somos mais baratos e menores nos
dois eixos.

O default de 64 vem de preservar o `SCAN_EF` fixo pré-M35 (`am/guc.rs:22`). O do pgvector é 40. Sessenta
e quatro põe a partida do scan ordenado acima do plano concorrente; quarenta a mantém abaixo.

## Por que era intermitente (1-em-11)

Em `ef=40` a margem é de 434,34 contra 567,66 — perto o bastante para que mudanças pequenas nas
estimativas de linha atravessem o fio. Nove arquivos de teste do `theo-rag` escrevendo em paralelo
contra um banco só movem exatamente isso. Não é aleatoriedade; é uma comparação apertada.

## O conserto, e o que ele custa

Baixar o default para 40 é de uma linha e **troca recall por plano**. `ef_search` é o botão de
recall/velocidade do scan; 64 foi escolhido para preservar comportamento, não medido contra 40. A
mudança exige recall@10 medido nos dois valores antes, no arnês — e é isso, não o número em si, que
decide.

## Nota de superfície, encontrada no caminho

O opclass do nosso AM chama-se `theodb_hnsw_cosine_ops`; `vector_cosine_ops` — o nome do pgvector — é
recusado com `operator class "vector_cosine_ops" does not exist for access method "theodb_hnsw"`. Uma
aplicação migrando do pgvector precisa reescrever o `CREATE INDEX`, e isso não está na compatibilidade
que o shim promete.

## Reprodução

```sql
-- ATENÇÃO: a subconsulta PRECISA ser correlacionada, senão vira InitPlan e TODAS as linhas recebem o
-- MESMO vetor. Foi assim que a primeira rodada desta medição se perdeu (ver § Retratações).
INSERT INTO embeddings (chunk_id, vector)
  SELECT 1+(i%3000),
         (SELECT array_agg(random())::real[]::vector(384)
            FROM generate_series(1,384) g WHERE g >= 0 OR i = i)   -- <<< `i = i` correlaciona
  FROM generate_series(1,3000) i;
SELECT count(DISTINCT vector) FROM embeddings;   -- tem de dar 3000, não 1
```

---

# § Retratações

## Retratação 2 — "a causa é o tamanho do índice" (a mais grave)

A versão anterior deste arquivo concluía que a causa do [[B-018]] era o nosso índice ocupar 1,78× (depois
corrigido para 1,52×) o disco do pgvector. **Estava errado, e a causa do erro foi dado degenerado.**

O `INSERT` usava `(SELECT array_agg(random()) ... FROM generate_series(1,384))` **sem correlação com a
linha externa**. O PostgreSQL iça isso para um InitPlan e o avalia **uma vez** — apesar de `random()`
ser volátil, porque a volatilidade está dentro da subconsulta e a subconsulta não depende de nada de
fora. Resultado medido: `count(DISTINCT vector) = 1` em 3000 linhas. **Todos os vetores eram o mesmo.**

Isso não é ruído; muda o objeto medido. O pgvector deduplica vetores idênticos num único elemento com
vários heap TIDs — medido por `pageinspect`: **854 tuplas de elemento para 2000 linhas**. Daí o índice
dele parecer pequeno, daí o crescimento superlinear por elemento (549 B/elem em N=1000, 1140 em N=4000)
e daí a variância entre builds que atribuí à aleatoriedade de nível.

Refeito com 3000 vetores **distintos**, o resultado **inverte**: 680 páginas nossas contra 751 dele.
Nosso índice é **menor**. Não há gap de tamanho a consertar, e o item [[B-092]], aberto para consertá-lo,
foi morto pela mesma medição.

**A hipótese de compressão foi testada e refutada** antes de eu achar a degenerescência: vetores em
[0,1) (expoentes repetidos) e vetores de expoente disperso deram 207 e 215 páginas no pgvector, 454 nas
duas no nosso. Não era compressão.

## Retratação 1 — o controle com pgvector 0.5.1

O primeiro controle usou pgvector **0.5.1**, que escolheu o HNSW em todos os casos, e teria concluído
"defeito nosso, o pgvector não tem". O `hnsw.c` do 0.5.1 tem um modelo anterior e muito mais cru:

```c
costs.numIndexTuples = (entryLevel + 2) * m;   /* ANTES do genericcostestimate */
*indexStartupCost = costs.indexTotalCost;      /* "most work happens before first tuple" */
```

O do 0.8.0 é o nosso. Comparar contra o 0.5.1 mediu a distância entre duas **gerações do modelo**, não
entre duas implementações dele.

## O que as duas retratações têm em comum

Nenhuma foi pega por leitura — as duas foram pegas por **um controle sobre a própria medição**: "essa
versão é a comparável?" e "esses dados são o que eu penso que são?". A segunda só apareceu porque um
número não fechava aritmeticamente (2000 vetores de 1544 B não cabem em 1,76 MB) e eu fui atrás em vez
de publicar. O hábito que salvou as duas foi desconfiar do número bom, não do ruim.

---

# Material da primeira rodada (preservado)

> **CORREÇÃO POR ACRÉSCIMO, 2026-08-21, algumas horas depois de publicar.** A razão de tamanho abaixo
> foi escrita como **1,78×** a partir de **uma** amostra de cada lado. O índice HNSW do pgvector varia
> entre builds (atribuição de nível é aleatória): medido três vezes, **444 / 416 / 482** páginas,
> CV de **7,4%**. O nosso é determinístico: **680 / 680 / 680**.
>
> A razão honesta é **1,52× (IC 95% [1,41×, 1,63×], p = 0,0067, n=3 de cada)** — Welch não-pareado com
> bootstrap sobre a razão, pela ferramenta do [[B-049]]. **A diferença continua real e significativa; a
> precisão que "1,78×" sugeria não era.**
>
> O corpo abaixo fica como estava, com os números daquela rodada. Eles não são falsos: para o par de
> índices presente naquele momento, a razão de custo (1,769×) casou com a razão de tamanho daqueles dois
> índices (680/382). O que estava errado era **generalizar de N=1** num eixo que varia 7%.
>
> É desconfortável registrar isto: o [[B-049]] entregou nesta mesma sessão a ferramenta que existe
> exatamente para não publicar razão a partir de uma corrida, e eu publiquei uma horas depois. Fica
> escrito porque o erro já tinha sido citado no `BACKLOG.md` e apagá-lo esconderia isso.

Peça relacionada: [pgrx](../technologies/pgrx.md) e o
[ADR-0065](../decisions/0065-b032-unsafe-op-marcado-por-operacao.md), que também trocou um número
herdado por um medido.

# O que reproduziu

O [[B-018]] registrava que o planner não alcança o HNSW no caminho de junção, e **não reproduziu em
seis cenários** em 2026-08-11 — parâmetro vs literal, generic plan, estatística ausente, ordem de
criação do índice. O sétimo cenário reproduz, deterministicamente:

```sql
SELECT e.id FROM embeddings e
  JOIN chunks c ON c.id = e.chunk_id
  JOIN documents d ON d.id = c.document_id
 WHERE d.tenant = 't1'                       -- <<< o filtro seletivo é o gatilho
 ORDER BY e.vector <=> $1 LIMIT 5;
```

Sem o `WHERE`, `embeddings` dirige e o HNSW serve a ordenação. **Com** ele, a ordem de junção inverte,
`embeddings` vai para o lado interno de um Nested Loop por `chunk_id`, e um `Sort` aparece — a forma
exata do relato original (`Limit → Sort → Nested Loop → Index Scan`).

O `Sort` não é evitável por knob: com `enable_sort = off` o plano continua o mesmo, marcado
`Disabled: true`. Não há caminho alternativo a gerar.

# A virada, medida

| `ef_search` | partida do HNSW | plano escolhido |
|---|---|---|
| 40 | 425,60 | **HNSW** (vence o Sort de 559,36 por 24%) |
| 56 | — | Sort |
| **64 (nosso default)** | — | **Sort** |

**A margem no melhor caso é 24%** — e é isso que explica a intermitência de 1-em-11 que o teste do
`theo-rag` declara e que seis cenários determinísticos não pegaram. Não é aleatoriedade: é uma
comparação no fio da navalha, e nove arquivos de teste escrevendo em paralelo contra um banco só
movem o custo do lado concorrente para os dois lados do fio.

# Uma retratação, registrada porque a conclusão errada quase entrou

O primeiro controle usou **pgvector 0.5.1**, que escolheu o HNSW em todos os casos — e a conclusão
seria "defeito nosso, o pgvector não tem". **Estava errada.** O `hnsw.c` do 0.5.1 tem um modelo de
custo anterior e muito mais cru:

```c
costs.numIndexTuples = (entryLevel + 2) * m;   /* ANTES do genericcostestimate */
*indexStartupCost = costs.indexTotalCost;      /* "most work happens before first tuple" */
```

O do **0.8.0** é o nosso: mesmo `ratio = (entryLevel*m + layer0TuplesMax*layer0Selectivity)/tuples`,
mesmo `layer0TuplesMax = HnswGetLayerM(m,0) * hnsw_ef_search`, mesmo `scalingFactor = 0.55`, mesmo
`startup = total × ratio`, mesma correção TOAST com as duas guardas. Nosso `am/cost.rs` é port fiel do
pgvector **atual**; comparar contra o 0.5.1 mediu a distância entre duas gerações do modelo, não entre
duas implementações dele.

# O controle justo, e a causa

pgvector **0.8.6**, mesmo esquema, mesmo dado, mesma query:

| motor | partida ef=40 | partida ef=64 | plano |
|---|---|---|---|
| pgvector 0.8.6 | 240,62 | **342,40** | **HNSW nos dois** |
| TheoDB | 425,60 | acima de 559 | HNSW só em 40 |

Mesma fórmula, custo **1,769×** maior. E a explicação não está na fórmula:

| | páginas do índice | páginas do heap | tuplas |
|---|---|---|---|
| TheoDB `theodb_hnsw` | **680** | 600 | 3000 |
| pgvector `hnsw` | **382** | 600 | 3000 |

**1,78× de páginas**, contra 1,769× de custo. O `genericcostestimate` cobra proporcionalmente a
páginas de índice, então a divergência de custo é consequência aritmética da divergência de tamanho.

**A causa do B-018 não é o modelo de custo** — que era a hipótese do item, herdada do `m175`. É o
índice ocupar 1,78× mais disco, agravado por um default de `ef_search` de 64 contra os 40 do pgvector,
que empurra o ratio ainda mais para cima.

# O que isso muda no alvo do conserto

O conserto mora no **layout de armazenamento do índice**, não em `am/cost.rs`. Mexer na fórmula de
custo para compensar o tamanho seria mentir para o planner sobre quanto o scan custa — e o custo
maior é verdadeiro: o índice é maior mesmo, e lê-lo custa mais mesmo.

Dois eixos, nesta ordem:

1. **Por que 680 contra 382** para vetores idênticos de 384 dimensões. Essa é a medição que ainda
   falta, e é a que decide o conserto.
2. **O default de `ef_search`** — 64 herdado do `SCAN_EF` fixo pré-M35, contra os 40 do pgvector.
   Baixá-lo é de uma linha e move a virada, mas troca recall por plano, o que exige medir recall antes.

# Reprodução

```bash
docker run -d --name c -e POSTGRES_HOST_AUTH_METHOD=trust ghcr.io/usetheoai/theo-db:develop
# tabelas documents/chunks/embeddings, 200/3000/3000 linhas, vector(384)
# CREATE INDEX ... USING theodb_hnsw (vector theodb_hnsw_cosine_ops)
# EXPLAIN da junção acima, com e sem o WHERE d.tenant
```

**Nota de superfície, encontrada no caminho:** o opclass do nosso AM chama-se
`theodb_hnsw_cosine_ops`; `vector_cosine_ops` — o nome do pgvector — é recusado com
`operator class "vector_cosine_ops" does not exist for access method "theodb_hnsw"`. Uma aplicação
migrando do pgvector precisa reescrever o `CREATE INDEX`, e isso não está na compatibilidade que o
shim promete.
