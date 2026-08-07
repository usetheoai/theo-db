---
type: Decision
title: ADR 0042 — Construir um Table Access Method colunar próprio (supersede o DEFER no caminho own-code)
description: Estudar os desenhos AGPL como literatura e reimplementar do zero em Rust/pgrx é legal e nunca esteve na cédula do ADR 0041 — mesma postura já ratificada no pilar vetorial.
resource: git:f7c7b93:docs/adr/0042-m99-own-code-columnar-tam.md
tags: [adr, columnar, table-access-method, own-code, licenca, clean-room, m99]
adr_id: "0042"
adr_status: Proposed
decision_date: 2026-07-14
milestone: M99
supersedes_in_part: ["0041"]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0042
    resource: git:f7c7b93:docs/adr/0042-m99-own-code-columnar-tam.md
    title: ADR 0042 — M99 own-code columnar TableAM
    last_modified: 2026-07-14
---

O [ADR 0041](/decisions/0041-m97-columnar-defer.md) pesou *adotar AGPL/BSL* contra *manter o
pg_duckdb*. **Não pesou construir permissivamente do zero** — a mesma opção que produziu o tipo
`vector` e os access methods próprios. Este ADR acrescenta essa terceira opção e a escolhe.

# Correção de honestidade

Uma emenda anterior de roadmap rotulara este trabalho como "modelo Hydra, Apache-2.0". **O subtree
colunar do Hydra é AGPLv3.** Este ADR, a edição do roadmap e uma entrada de CHANGELOG corrigem o erro
factual. A única referência colunar nativa em Apache-2.0 é um FDW — não um Table Access Method — e
está descontinuado.

# Decisão

Construir um `theodb_columnar` como `TableAmRoutine` em Rust/[pgrx](/technologies/pgrx.md), do zero:
**estudar Hydra e Citus como literatura de desenho; não copiar fonte AGPL e não linkar biblioteca
AGPL.** As superfícies permissivas de reuso são o FDW Apache-2.0 e o [arrow-rs](/technologies/arrow.md).

**Escopo:** analítico **append-only** — INSERT, COPY, seq-scan, agregação e index-fetch. Callbacks de
UPDATE, DELETE, tuple-lock, serializável, paralelo, bitmap e sample são stubs com erro tipado.
"Columnar HTAP atualizável" está **explicitamente fora de escopo** — reivindicá-lo seria over-claiming.

Reusa a superfície de storage já existente do projeto: páginas com WAL, codec de TID e o idioma de
registro de AM. **Não é greenfield.**

# Racional

1. **Own-code nunca esteve na cédula do 0041.**
2. **É legal.** Algoritmos e layouts on-disk não são protegidos por copyright; uma reimplementação
   clean-room em Rust sobre pgrx e arrow-rs não viola licença. O precedente é o pilar vetorial contra
   o VectorChord AGPL.
3. **O truque de correção de MVCC é reusável de graça.** Delegar a visibilidade de stripe ao MVCC de
   uma linha de catálogo heap — o desenho de Citus e Hydra — significa que **não reimplementamos
   MVCC**, que é a coisa de maior risco que um TAM poderia fazer. Mantém a complexidade essencial
   essencial e a correção nativa do Postgres.
4. **Diferencia da rota embarcada.** O pg_duckdb é engine e planner *separados*; um TableAM nativo é
   **storage in-core do PG** — o substrato que o pilar de planner único precisa para empurrar scans
   para dentro. Este é a metade de storage dessa costura.[^adr0042]

# Alternativas rejeitadas

**Manter o DEFER** — deixaria o pilar de planner único sem substrato colunar nativo. **Adotar Hydra
ou Citus** — AGPLv3, barrados. **Parquet por stripe em vez de layout próprio** — *adiada, não
rejeitada*: trocaria integração nativa de WAL e crash-safety por reuso de formato; a escolha foi
framing próprio em fork do PG mais os **codecs** do arrow-rs, de modo que a crash-safety fica nativa e
a compressão é reusada. **Páginas de metadados com visibilidade escrita à mão** — reimplementaria
MVCC, e descartaria justamente o truque de correção.

# Consequências

Entrega um TableAM colunar **append-only**; UPDATE e DELETE são aposta posterior, e sua única
implementação de referência é AGPL, logo teria de ser desenhada clean-room.

Stripes abortados vazam espaço em disco até uma reescrita — aceito e documentado, igual ao
precedente. Não há isolamento serializável no colunar, registrado como não-objetivo explícito.

E a prova de correção **não é de-riscável sem testes de permutação de concorrência** — que são item
obrigatório do critério de pronto, não um "seria bom ter".

O resultado dessa linha, e a remoção completa do pg_duckdb que ela viabilizou, estão no
[ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md).

[^adr0042]: ADR 0042 — M99: build an OWN-CODE columnar Table Access Method
