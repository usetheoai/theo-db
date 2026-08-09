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

## B-001 — `cargo pgrx test` não roda: o binário de teste morre em `CurrentMemoryContext`   [ ]

domain: engine-pgrx
repo: theo-db
suggested_mode: bug
source: human
evidence: reproduzido em 2026-08-09 no builder do próprio `Dockerfile` — `cargo pgrx test pg18 <filtro>` falha com `symbol lookup error: undefined symbol: CurrentMemoryContext`
why_now: a suíte tem 310 testes e **nenhum deles roda localmente** pelo caminho documentado. Descoberto ao tentar validar 6 testes novos do `parquet.rs`; confirmado como **pré-existente** rodando `cargo pgrx test pg18 sq8` — teste que existe desde antes — com todas as mudanças da sessão revertidas via `git stash`. Uma suíte que só roda no CI é uma suíte cuja regressão só aparece depois do push.
status: raw
dod:
  - `cargo pgrx test pg18 parquet` executa e reporta resultado de teste (passou ou falhou), em vez de morrer no carregamento
  - a correção é verificada num teste pré-existente (`sq8`), não só nos testes novos
  - o caminho que funcionar fica documentado em `scripts/pgrx-test-in-builder.sh`, que hoje descreve uma receita que não chega a executar

> Registered 2026-08-09 by `/backlog-item` (slug: `theo-db-pgrx-test-nao-executa`).

**Três hipóteses já testadas e REFUTADAS** — registradas para que ninguém as repita:

| # | hipótese | resultado |
|---|---|---|
| 1 | falta `.cargo/config.toml` com `-Wl,--unresolved-symbols=ignore-all` | **muda o sintoma, não resolve**: o link passa a completar, e a falha migra de *link-time* (`undefined symbol: FreeErrorData/FlushErrorState/pfree`) para *runtime* (`CurrentMemoryContext`). É progresso diagnóstico, não correção |
| 2 | o bootstrap `pub mod pg_test` está sob `#[cfg(test)]` enquanto os 56 módulos de teste usam `cfg(any(test, feature = "pg_test"))` | **sem efeito.** O desalinhamento é real e provavelmente deve ser corrigido de qualquer forma, mas não é a causa |
| 3 | `crate-type = ["cdylib"]` sem o `"lib"` que o template do pgrx traz | **sem efeito** (rebuild confirmado por mudança de hash do binário) |

**O que a evidência sugere:** o binário de teste está sendo executado como processo standalone em vez de ser
carregado pelo Postgres. Nenhuma das três hipóteses toca esse ponto, e é por aí que a próxima investigação
deveria começar.

Próximo id livre: **`B-002`**. Ids são monotônicos e nunca reusados.
