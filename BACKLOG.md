# BACKLOG — TheoDB

Registro de trabalho de manutenção deste repositório: uma linha por hipótese, com dono e critério de
fechamento. **Ids são monotônicos, nunca reusados, nunca renumerados** — um item morto guarda o número
dele para sempre, porque o número é o rastro.

Um item aqui é **hipótese, não compromisso**. Ele nasce com `evidence: none-yet` por desenho; provar ou
matar é trabalho do `/discover`.

## Divergência declarada do `cycle-backlog.md`

A regra desenha **um registro único no umbrella**, cobrindo os 21 repositórios, e o `/backlog-init`
recusa dentro de um repo governado — *"a per-repo backlog is the fragmentation the single registry exists
to prevent"*.

**Este arquivo diverge disso, por decisão do owner em 2026-08-08.** Duas razões o sustentam:

1. O `/backlog-init` está **bloqueado no umbrella** — `theo-platform/` não tem `CHANGELOG.md`, e o Step 0.3
   falha. Foram quatro invocações recusadas antes desta decisão.
2. `knowledge-base-location.md` estabelece que **consumidores não compartilham knowledge-base**: cada
   projeto é dono do próprio `ROADMAP.md` e do próprio `.claude/knowledge-base/`. Um backlog local é
   coerente com essa autonomia; o que ela proíbe é um artefato de ciclo referenciar outro repositório.

O custo da divergência é real e fica dito: **um achado que cruze repositórios não tem onde morar aqui.**
Quando o umbrella ganhar `CHANGELOG.md`, este arquivo deve ser reavaliado — migrado ou mantido com escopo
explícito.

## Fronteira com o `ROADMAP.md`

Os dois existem e **não são intercambiáveis**:

| | vai para |
|---|---|
| trabalho de produto, com marco e Definition of done | **`ROADMAP.md`** (`M<N>`) — é o que `cycle-release` e `cycle-acceptance` leem |
| hipótese de manutenção, ainda sem evidência | **este arquivo** (`B-NNN`) |

Um item que já tem escopo de milestone nasce no roadmap, não aqui. Um item que precisa ser **medido antes
de virar escopo** nasce aqui e sobe depois.

## Como um item chega

- **`/backlog-item {slug}`** — humano, hipótese sem evidência
- **`/discover --sweep {pilar}`** — achado já medido, entra com `source: discover-*`

Schema, transições de status, gates G1–G5, vereditos e rollback: `.claude/rules/cycle-backlog.md`. Este
arquivo é dado; o contrato vive na regra.

## Roteamento por pilar

O escopo é um repositório, então o `domain` do schema é **o pilar**, não um repo do ecossistema. Cada um
tem especialista registrado em `.claude/agents/` (fora do versionamento — ver `.gitignore:14`).

| Pilar | Superfície | Especialista |
|---|---|---|
| `vetorial` | `theodb_rs/src/ann/`, `src/vec/`, quantização, recall | `theo-recall` |
| `hot-path` | `src/ann/scan_core.rs`, `src/am/page/`, SIMD, layout | `theo-hotpath` |
| `concorrencia` | `src/am/lock.rs`, `am/scan.rs`, build paralelo, HTAP | `theo-concurrency` |
| `colunar` | `src/am/columnar*.rs`, zonemap, DataFusion, Parquet | `theo-columnar` |
| `lexical` | `lexical_core/`, `src/lexical/`, `src/hybrid.rs`, BM25/RRF | `theo-lexical` |
| `ai-surface` | `src/{ai_op,chat,embed,rerank,nl,vectorizer,egress}.rs`, `sql/` | `theo-ai-surface` |
| `engine-pgrx` | FFI, `unsafe`, crash-safety, superfície SQL, upgrade | `theo-pgrx` |
| `acervo` | `wiki/`, conceitos OKF, proveniência | `theo-wiki` |
| *(transversal)* | auditoria de qualquer número publicado | `theo-auditor` |

Um item que abranja dois pilares **é dois itens** (gate G3).

## Items

*(vazio — próximo id livre: `B-001`)*

Semeado sem itens, por desenho: um item que ninguém filou não tem `why_now`, não tem DoD e não tem dono —
é um placeholder que será herdado como se fosse decisão. A medição de maturidade dos pilares, que motivou
a criação deste arquivo, já existe como **M184** no `ROADMAP.md`, porque nasceu com escopo de milestone.
