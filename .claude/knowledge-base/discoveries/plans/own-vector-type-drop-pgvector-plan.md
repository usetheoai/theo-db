# Discovery Plan: Tipo vetorial próprio — remover a dependência do pgvector totalmente

> **Version 1.1** — (v1.1 absorveu o edge-case review 2026-07-09: EC-1 MUST-FIX — o padrão pgrx de DEFINIÇÃO do tipo é R0-web-sourced, pois nenhum peer permissivo shipa tipo próprio em pgrx; + checkpoints EC-2/EC-3 para a dependência Q6→Q1 e o fallback de migração.) Investiga como shipar um **tipo `vector` próprio own-code** no theodb_rs (I/O, typmod, operadores de distância, casts, operator classes ligadas aos AMs próprios `theodb_hnsw`/`theodb_ivfflat`) para **remover totalmente** a dependência do pgvector (e do pgvectorscale, vestigial no runtime). Referências in-scope: `pgvector` (impl de referência legal — PostgreSQL license), `vectorchord` e `pgvectorscale` (dois AMs próprios SOTA que **reusam** o tipo do pgvector — evidência-chave para a decisão de nome/compat), e `postgres` core (padrão custom-type + opclass, ex. `contrib/cube`). Output: um blueprint que **recomende A (tipo próprio nomeado `vector`, drop-in) vs B (`theodb.vector`, namespaced)** com evidência, descreva o padrão de I/O/operadores/opclass em pgrx 0.16.1, o caminho de migração de tabelas existentes, o gate de paridade/correção, e a decomposição honesta em 1 vs 2 milestones.

**Slug:** `own-vector-type-drop-pgvector`
**Owner:** paulohenriquevn
**Created:** 2026-07-09
**Time budget:** 9h (breakdown em ADR D1)

## Context

O usuário exige **remover a dependência do pgvector TOTALMENTE** (2026-07-09). É objetivo explícito do North Star do projeto (`theo-db/CLAUDE.md` § "North Star": *"Substituir terceiros (pgvector/pgvectorscale/plpython3u) por código próprio é o **objetivo**"*) e o alvo dos milestones v2 M20→M22, que foram **gated em paridade medida sem regressão** (`ROADMAP.md` § "Sequência e paralelismo": *"M20→M22 reduz/elimina pgvector/pgvectorscale — cada passo gated por paridade medida"*).

Evidência do acoplamento atual (grep 2026-07-09, este repo):
- **Não existe tipo próprio.** `theodb_rs/src/api.rs:770` (comentário): *"M20 ... Coexists with pgvector (reads `vector::real[]`; no competing type)"*. O ADR-0006 fixou o reuso do tipo do pgvector (Regra 9). Este discovery investiga **superseder** o ADR-0006.
- **Superfície:** `theodb.control` (`requires = 'vector, vectorscale'`), `theodb_rs/theodb_rs.control` (`requires = 'vector, theodb'`); ~105 refs `::vector`/`vector(` no Rust de produção; opclass/AM DDL em `theodb_rs/src/am/mod.rs`; operadores `<->`/`<=>`/`<#>` são do pgvector (`theodb_rs/src/am/autotune.rs`, `build.rs`); mecanismo M49 metric-from-opclass em `theodb_rs/src/am/build.rs`.
- **vectorscale é vestigial no runtime:** nenhum caminho de produção usa `USING diskann` (só `benchmarks/`); tirá-lo do produto é barato.

Regras do projeto que emolduram o discovery: **Regra 9 / `parsimony-ladder.md`** (não reinventar — mas o North Star declara que substituir o pgvector é complexidade **essencial**, não acidental; o discovery deve pesar isso honestamente contra o custo), **`architecture.md`** (o tipo próprio é uma fronteira nova — DIP: o domínio define, o AM consome), **`testing.md`** (o gate de paridade/correção é onde a suíte existente que usa `vector` deve continuar verde), **`discover-phd-rigor.md` R0** (busca web obrigatória com citação em todo o discover).

## Objective

**O blueprint deve permitir decidir:** vale a pena e como shipar um tipo `vector` próprio own-code para remover o pgvector totalmente — com uma recomendação fundamentada A (drop-in `vector`) vs B (`theodb.vector`), o padrão técnico de implementação em pgrx 0.16.1, o gate de correção, o caminho de migração, e a decomposição em milestones.

Critérios de sucesso mensuráveis do blueprint:

- [ ] Todas as research questions respondidas com citação a `.claude/knowledge-base/references/`
- [ ] Tabela comparativa cross-cutting preenchida para cada referência in-scope (pgvector, vectorchord, pgvectorscale, postgres)
- [ ] Recommendations com ≥ 1 proposta de decisão concreta por research question (incl. o veredito A vs B com evidência)
- [ ] **Cada afirmação SOTA/comparativa carrega evidência web (R0: papers/OSS/blogs) OU marcador `UNBENCHMARKED`** (`discover-phd-rigor.md` R0/R3)
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvector/` | `src/vector.c`, `src/vector.h`, `sql/vector.sql`, `src/hnsw.c` | **Referência legal (PostgreSQL license, Regra 9)** do tipo `vector`: I/O in/out/recv/send, typmod (dim), validação NaN/Inf, operadores, e a DDL completa (CREATE TYPE/OPERATOR/OPERATOR CLASS) que o tipo próprio deve espelhar; `hnsw.c` = binding opclass↔AM no lado C. |
| `.claude/knowledge-base/references/vectorchord/` | `vchord.control`, `src/lib.rs`, `src/datatype/` | AM próprio SOTA. `vchord.control` diz `requires = 'vector'` → **reusa** o tipo do pgvector. `src/datatype/` (memory_vector.rs etc.) — investigar: codec interno sobre o tipo público OU tipo competidor? Evidência-chave da decisão A vs B. |
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/src/access_method/pg_vector.rs`, `.../guc.rs` | Como um **AM custom em pgrx** consome o tipo `vector` (também `requires vector`). O padrão pgrx 0.16.1 já parcialmente adotado (`theodb_rs/src/am/guc.rs:5` cita guc.rs deste projeto). |
| `.claude/knowledge-base/references/postgres/` | `contrib/cube/cube.c`, `contrib/cube/cube--1.2.sql` | Padrão textbook do core: custom type C (I/O + operadores) + operator class GiST via DDL — o esqueleto CREATE TYPE→CREATE OPERATOR→CREATE OPERATOR CLASS que qualquer tipo próprio segue. |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/pgvector/test/`, `pgvectorscale/**/test*` (exceto quando Q5 os cita) | Só lidos quando uma question de tests/paridade os aponta; não varredura geral. |
| `.claude/knowledge-base/references/{citus,cloudnative-pg,paradedb,duckdb,hydra,pg_mooncake,patroni,pgbackrest,pinecone-python-client,mcp-go-sdk,supabase-postgres}/` | Fora do tema (não são impl de tipo vetorial). |
| `halfvec`/`bit`/`sparsevec` do pgvector (`src/halfvec.c`, `src/bitvec.c`, `src/sparsevec.*`) | O escopo é o tipo `vector` (float4) — half/bit/sparse são tipos adicionais, YAGNI para a remoção do pgvector (deferíveis a follow-up). |
| Qualquer projeto NÃO clonado em `.claude/knowledge-base/references/` | Cross-Project Rule: nunca afirmar feature sem ler a fonte. |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvector: 4h (referência primária — o tipo a espelhar); vectorchord: 2h (decisão A vs B); pgvectorscale: 1.5h (padrão pgrx custom-AM + o que sobra ao remover); postgres core: 1.5h (esqueleto DDL custom-type+opclass).

**Rationale:** o pgvector é a fonte legal e mais completa do tipo `vector`, logo o dive mais fundo; vectorchord/pgvectorscale respondem a decisão de nome/compat e o padrão pgrx; postgres core dá o esqueleto DDL canônico. Alternativas consideradas: split igual (rejeitado — pgvector merece o dobro), single-project pgvector-only (rejeitado — perderia a evidência A vs B do vectorchord).

**Stop condition — per question (mandatory):** quando a Fase A de uma question retorna vazio após 3 retries com variantes de query, marcar BLOCKED com razão "Fase A exhausted" e seguir. NUNCA preencher com hotspots de outra question.

**Stop condition — per project (mandatory):** budget exausto com N questions pendentes → marcar as remanescentes BLOCKED "budget exhausted". Se todas as questions estão `done` ou honestamente `blocked`, emitir `<promise>BLUEPRINT_BLOCKED</promise>` (não COMPLETE) com o relatório honesto.

**Anti-pattern:** NUNCA fabricar respostas Fase B para uma question com Fase A exausta. BLOCKED honesto com razão é obrigatório (Regra 3).

**Consequences:** o halt-loop para de iterar num projeto quando o budget acaba; questions bloqueadas viram seed do próximo discovery.

### D2 — Investigation depth + R0 web mandate

**Decision:** para cada question, Fase A (grep/find — mapa de hotspots no código clonado) + Fase B (Read fundo). **ADICIONALMENTE (R0, obrigatório):** cada question de `techniques`/`tools` DEVE complementar a leitura do código com **WebSearch/WebFetch** (papers, docs oficiais, blogs — restrito ao `discover-web-allowlist.txt`) e citar — conhecimento interno + leitura local NÃO bastam (`discover-phd-rigor.md` R0).

**Rationale:** o código clonado responde "como É"; a web responde "qual é o SOTA e o rationale" (ex.: por que VectorChord/pgvectorscale reusam o tipo em vez de reimplementar — há blog/issue documentando?). Alternativa (só código local) rejeitada por R0.

**Consequences:** cada answer de techniques carrega ≥ 1 fonte web + ≥ 1 citação de referência clonada. Afirmações de custo/esforço marcadas honestamente (essencial vs acidental).

### D3 — Supersede vs coexist (framing do resultado, não decisão de código)

**Decision:** o blueprint TRATA a premissa criticamente — se a evidência (vectorchord + pgvectorscale reusam `vector`) indicar que "remover totalmente" tem custo desproporcional vs valor, o blueprint DEVE dizê-lo honestamente (Regra 3), oferecendo o caminho + o custo, não uma torcida pela remoção.

**Rationale:** discover não conclui a decisão de produto; ela informa. Honestidade extrema (Regra 3) sobre o fato de que os dois AMs próprios SOTA reusam o tipo é obrigatória. Alternativa (assumir que remover é sempre certo) rejeitada — seria discovery-theater.

**Consequences:** o veredito A vs B pode incluir uma terceira via ("C — manter reuso, remover só o vectorscale") se a evidência a sustentar; o usuário decide no plan.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (mapa) | Fase B (Read fundo) + R0 web | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | Como o pgvector implementa o tipo `vector`: I/O in/out/recv/send (parse text `[1,2,3]` + binário), typmod (dimensão), validação (NaN/Inf reject, dim mismatch), e o layout do struct `Vector`? | techniques | `pgvector/src/vector.c`, `src/vector.h` | Grep `vector_in\|vector_out\|vector_recv\|vector_send\|CheckDim\|CheckElement\|typmod` em `vector.c/.h`; Read cada função. **R0:** docs/blog do pgvector sobre o formato do tipo. | Tabela função→responsabilidade→regra-de-validação com `pgvector/src/vector.c:linha`; o contrato exato a espelhar own-code. |
| Q2 | Como se liga um custom type a um custom AM: operadores `<->`/`<=>`/`<#>`, operator classes, e support procs (amproc FUNCTION 1 distance; ORDER BY op) — em C (pgvector/postgres) e o análogo em pgrx 0.16.1? | techniques | `pgvector/sql/vector.sql`, `pgvector/src/hnsw.c`, `pgvectorscale/pgvectorscale/src/access_method/pg_vector.rs`, `postgres/contrib/cube/cube--1.2.sql` | Grep `CREATE OPERATOR\|OPERATOR CLASS\|FUNCTION 1\|amproc` em `vector.sql` e `cube--1.2.sql`; Read o binding opclass↔AM em `hnsw.c` + como `pg_vector.rs` lê o tipo em pgrx. **R0 (EC-1 — obrigatório, não opcional):** o padrão pgrx da **DEFINIÇÃO do tipo** (I/O in/out/recv/send, typmod, `extension_sql! CREATE TYPE`) NÃO tem fonte local — os clones REUSAM o tipo do pgvector (C). Fonte = **pgrx book/examples + pg docs "User-defined Types" + "Interfacing Extensions To Indexes"** (web, allowlist). Os clones dão o **contrato/semântica** (pgvector C) e o **consumo** por um AM pgrx (pgvectorscale). | Esqueleto DDL CREATE TYPE→OPERATOR→OPERATOR CLASS + o mapeamento para pgrx 0.16.1 (web-sourced), com citações; como preservar o M49 metric-from-opclass. **Finding esperado:** nenhum peer permissivo shipa tipo próprio em pgrx (reforça "território novo, custo real"). |
| Q3 | VectorChord e pgvectorscale **reusam** o tipo `vector` do pgvector ou shipam próprio? Qual o rationale? `vchord/src/datatype/` é tipo público competidor ou codec interno? — evidência para A (drop-in `vector`) vs B (`theodb.vector`). | techniques | `vectorchord/vchord.control`, `vectorchord/src/lib.rs`, `vectorchord/src/datatype/mod.rs`, `pgvectorscale/pgvectorscale/src/access_method/pg_vector.rs` | Read `vchord.control` (`requires`); Grep `CREATE TYPE\|pgrx.*PostgresType\|SqlTranslatable` em `vectorchord/src`; Read `datatype/mod.rs`. **R0:** blog/README/issues VectorChord & pgvectorscale sobre por que reusam vs reimplementam o tipo; risco real de colisão de nome. | Veredito com evidência: reusam/reimplementam + rationale; recomendação inicial A vs B vs C (manter reuso) com trade-offs de colisão de nome e migração. |
| Q4 | Como provar **paridade/wire-compat 100%** do tipo próprio vs pgvector (formato text/binário idêntico, semântica dos operadores, typmod) — e como o pgvector testa isso (regress)? | tools | `pgvector/test/` (sql/expected), `pgvector/sql/vector.sql` | Find `pgvector/test/sql/*.sql` + `expected/*.out`; Read os casos de I/O/operadores/typmod. **R0:** docs pg "regress tests" + qualquer guia de binary-compat de tipos. | O conjunto de casos de paridade (I/O round-trip, NaN/Inf, dim-mismatch, operadores, typmod) que o gate de correção own-code deve replicar; comando de teste. |
| Q5 | Como pgvector/pgvectorscale testam o tipo **end-to-end** (round-trip I/O, operadores, index scan sobre o tipo via o AM), e o que isso implica na decomposição em **1 vs 2 milestones**? | tests | `pgvector/test/sql/`, `pgvectorscale/pgvectorscale/src/access_method/` (mod tests) | Find os testes de index-scan-sobre-o-tipo em pgvector/pgvectorscale; Read o shape (build índice → query com operador → assert recall/ordem). **R0:** — (shape de teste, código local basta; R0 opcional). | Shape dos testes de integração do tipo próprio (I/O + operador + index scan via theodb_hnsw) + proposta honesta de decomposição M(N) tipo+operadores / M(N+1) opclasses+remoção+migração, OU milestone único. |
| Q6 | O que sobra em `requires`/Dockerfile ao remover pgvector+pgvectorscale, e qual o caminho de **migração de tabelas de usuário** com colunas `vector` existentes (ALTER TYPE / binary-compat / recast) sem perda; mover diskann para benchmark-only? | deps | `pgvector/sql/vector--*.sql` (precedente de ALTER no tipo), `theodb.control`, este repo `Dockerfile` | Read exemplos de migração `ALTER`/`UPDATE` nas migrations do pgvector; Read `theodb.control`/`Dockerfile` (o que remover). **R0:** docs pg `ALTER TYPE`/binary-coercible + como extensões migram tipos entre versões. | Lista do que zerar (requires, stages do Dockerfile) + estratégia de migração de tabela existente (drop-in binário se layout idêntico ao pgvector? recast?) + plano diskann benchmark-only. |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| tests (integration) | Q5 | Covered |
| Dependencies | Q6 | Covered |
| Tools | Q4 | Covered |
| Techniques | Q1, Q2, Q3 | Covered (R4 phd-rigor: ≥2 em techniques ✓) |

**Coverage: 4/4 corners covered (100%)**

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | Todo `.claude/knowledge-base/references/{project}/{path}` da Fase A existe | Marcar Qx BLOCKED "path not found"; seguir |
| R0 gate (Q1,Q2,Q3,Q6) | A answer cita ≥ 1 fonte web (allowlist) além da referência clonada, OU marca `UNBENCHMARKED` | Re-iterar Qx buscando web (1 retry); senão BLOCKED "R0 unmet" |
| Per-question Fase A budget | Fase A ≥ 1 hotspot OU 3 retries de variante | Após 3 retries vazios, BLOCKED "Fase A exhausted"; seguir |
| After answering Qx | Seção do blueprint sob Qx tem ≥ 1 citação de referência | Re-iterar Qx (1 retry) |
| Per-project time budget | Budget do projeto não exausto | Exausto → remanescentes BLOCKED "budget exhausted"; próximo projeto |
| Q6 depende de Q1 (EC-2) | Antes de responder Q6, Q1 está `done` E a resposta de Q6 cita o layout do struct `Vector` de Q1 (varlena + int16 dim + float4[]); layout own-code byte-idêntico ⇒ migração drop-in binária, senão recast | Responder Q1 primeiro; se Q6 não citar o layout, re-iterar Q6 (1 retry) |
| Q6 fallback de migração (EC-3) | Se `pgvector/sql/vector--*.sql` não tem ALTER que mude o layout do tipo, NÃO marcar Fase A exausta | Fallback R0 web: pg docs binary coercibility / `CREATE CAST ... WITHOUT FUNCTION` como fonte do caminho de migração |
| Before promising complete | Os 4 corners têm seções populadas + o veredito A/B/C está presente com evidência | Recusar promise; continuar |

## Acceptance Criteria

- [ ] Todas as research questions respondidas OU explicitamente BLOCKED com razão
- [ ] Cada citação respaldada por um path real em `.claude/knowledge-base/references/` (hard cap: citação fabricada → INVALID)
- [ ] R0 honrado: Q1/Q2/Q3/Q6 com fonte web citada (ou `UNBENCHMARKED`/`BLOCKED R0` honesto)
- [ ] Os 4 corners do blueprint populados
- [ ] Veredito **A (drop-in `vector`) vs B (`theodb.vector`) vs C (manter reuso, remover só vectorscale)** com evidência e trade-offs
- [ ] Decomposição honesta em 1 vs 2 milestones proposta
- [ ] ≥ 1 ADR no blueprint (superseder ADR-0006)

## Global Definition of Done

- `/discover-edge-cases own-vector-type-drop-pgvector` rodado; MUST-FIX absorvidos (v1.1).
- `/discover-plan-confidence own-vector-type-drop-pgvector` ≥ SHIPPABLE_WITH_CAVEATS (hard caps: coverage corners não-vazios, citações resolvem, budget ≤ 15 questions — 6 aqui).
- Depois: `/discover-execute` (halt-loop) → `/discover-confidence` → o blueprint alimenta `/roadmap-feature` (M69[+M70]) → `/to-plan`.
- Thresholds/golden rule: `.claude/rules/discover-plan-golden-rule.md`, `.claude/rules/discover-blueprint-golden-rule.md`, `.claude/rules/discover-phd-rigor.md`.
