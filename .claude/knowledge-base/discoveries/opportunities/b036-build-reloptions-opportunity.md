---
item: B-036
mode: bug
date: 2026-08-13
verdict: pending
---

# B-036 — `m` já é per-índice e a varredura já o honra; falta o build ler da relação

## Corner 1 — Evidence

### O sintoma

Medido em 2026-08-12 contra `theodb:b034`, e reconfirmado no `theodb:b044`:

```
CREATE INDEX ... USING hnsw (embedding vector_l2_ops) WITH (m=16, ef_construction=64)
→ ERROR: unrecognized parameter "m"
```

Idem para `ef_construction` e `max_connections`, e idem nos AMs próprios `theodb_hnsw` / `theodb_ivfflat`. As
reloptions que de fato existem (`am/options.rs:112-196`): `lists`, `sbq_bits`, `pq_subspaces`, `pq_bits`,
`aq_threshold`, `separate_storage`, `refine`, `soar_lambda`, `rabitq_bits`.

O build usa constantes: `HNSW_M = 16` e `HNSW_EF_CONSTRUCTION = 64` (`am/build.rs:22-23`), esta última
sobreponível só por variável de ambiente do **servidor** (`build.rs:30-36`), inalcançável por sessão.

### O achado que muda o tamanho do trabalho

**`m` já é uma propriedade por índice, persistida e honrada pela varredura.**

| Fato | Onde |
|---|---|
| O meta de página tem `m: u16` e `m0` | `am/hnsw_page/meta.rs:40` |
| A busca lê `meta.m`, não a constante | `hnsw_page/search.rs:186,379,454` |
| O empacotamento também | `hnsw_page/store.rs:297,377` |
| O scan idem | `am/scan.rs:513` |
| A serialização do índice em memória grava `m`, `m0` e `ef_construction` | `ann/hnsw.rs:422-424` |

Ou seja: o caminho de **leitura** já respeita o que foi gravado no índice. O que falta é o caminho de
**escrita** ler da relação em vez da constante. Isso reduz o item de "propagar um parâmetro novo pelo
sistema" para "ligar a ponta que falta" — e faz com que **índices existentes continuem corretos por
construção**, porque cada um já carrega o próprio `m`.

### O que NÃO está persistido, e é onde mora o risco

`ef_construction` **não está no meta de página**. Está apenas no `to_bytes` do `HnswIndex` em memória, que é
outra serialização.

Isso importa num lugar só, e é o que decide o desenho: **o fold do VACUUM reconstrói o grafo**
(`am/build.rs:706`), e o comentário lá diz textualmente *"HNSW build params are fixed consts (no reloption),
so rebuild with them"*. Com reloptions de verdade e sem persistir `ef_construction`, um VACUUM reconstruiria
o índice com um valor diferente do de criação — silenciosamente. É a classe do [[B-048]] num lugar novo.

O padrão correto já está estabelecido no mesmo arquivo, para o `sbq_bits`:

> *"A fold reads this off the persisted meta (not the reloption), so this is only the initial-build gate."*
> (`am/options.rs`, docstring de `sbq_bits_from_relation`)

E o meta tem **discriminador de versão** para evoluir (`HNSW_STRUCT_VERSION_SBQ=2`, `_AQ=3`, `_V4=4`,
`meta.rs:89-91`) — o mecanismo de acrescentar campo já existe e já foi exercitado quatro vezes.

### A coincidência que preserva as medições publicadas

Os defaults do TheoDB (`m=16`, `ef_construction=64`) são **exatamente** os defaults do pgvector. Manter
esses defaults significa que toda corrida já publicada (`b035`, `b040`, `b044`, `b047`) continua sendo o
ponto de operação padrão, e nada precisa ser remedido.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `am/options.rs` | +2 reloptions, +2 campos no struct, +2 entradas na tabela de parse, +2 acessores |
| `am/build.rs` | os dois call sites (`:416` build inicial, `:706` fold) passam a ler da relação / do meta |
| `am/hnsw_page/meta.rs` | +1 campo `ef_construction`, com bump de versão do meta |
| Varredura | **nenhuma mudança** — já lê `meta.m` |
| Índices existentes | **continuam corretos**: cada um carrega o próprio `m`; sem a opção gravada, o default é o mesmo de hoje |
| `THEODB_HNSW_PARALLEL_THRESHOLD` / `THEODB_HNSW_EF_CONSTRUCTION` | a segunda vira redundante — duas fontes para o mesmo knob é a armadilha de precedência que o [[B-034]] pagou para resolver |
| [[B-046]] | **destravado** — passa a ser possível variar qualidade de grafo mantendo a varredura fixa |
| [[B-042]] | idem: o experimento que separa build lento de grafo pior passa a existir |

## Corner 4 — Verification

1. `CREATE INDEX ... WITH (m=32, ef_construction=200)` cria, e o índice **honra** os valores — provado por
   recall medido diferente entre dois `ef_construction`, não por o `CREATE INDEX` ter sido aceito.
2. `meta.m` do índice criado com `m=32` **é** 32 — lido de volta do disco.
3. Um VACUUM que dispare o fold reconstrói com os parâmetros **de criação**, não com os defaults nem com o
   reloption corrente — teste que hoje não teria como falhar porque a variação não existe.
4. Índice criado sem as opções continua abrindo, varrendo e dando o mesmo recall de hoje.
5. Faixa validada: valor fora dos limites é recusado no `CREATE INDEX`, não truncado em silêncio.

## Reclassificação

`suggested_mode: bug` mantido — o `ADR-0029 § D2` promete drop-in "sem mudança de código" e a sintaxe de
build do pgvector falha. O que a descoberta mudou é o **tamanho** (a metade da leitura já existe) e o
**risco** (o fold precisa do `ef_construction` persistido, ou reconstrói diferente em silêncio).
