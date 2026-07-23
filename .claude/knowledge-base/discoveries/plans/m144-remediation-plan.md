# Discovery Plan — m144-remediation

**Version:** 1.0
**Slug:** `m144-remediation`
**Owner:** engineering
**Created:** 2026-07-23
**Cycle:** discover (phase 1) → feeds `/to-plan m144`

## Context

O loop-code-review full de `theodb_rs/` (2026-07-23, `.claude/knowledge-base/audits/theodb-rs-code-review-2026-07-23.md`, 100 findings, 0 CRITICAL) achou 3 HIGH acionáveis que atingem o binário shipado. A investigação preliminar (in-repo, já executada) refinou cada um:

1. **Cadeia de upgrade congelada.** A superfície lakehouse M143 (`public.read_parquet`/`write_parquet`/`olap`, `theodb_rs/src/parquet.rs:76,122,169`) existe no Rust e no fresh-install (via `extension_sql!` REVOKE em `parquet.rs:323-325`), mas **não** no script hand-written `theodb_rs/sql/theodb_rs--1.0.0--1.1.0.sql` (0 ocorrências). Quem instalou 1.0.0 e roda `ALTER EXTENSION theodb_rs UPDATE` (prometido em `README.md:102`) não recebe a superfície. `default_version = '1.1.0'` (`theodb_rs/theodb_rs.control`).
2. **`symqg_spike_bench` PUBLIC.** Criado em `theodb_rs/sql/theodb_rs--1.0.0--1.1.0.sql:340` sem REVOKE; o loop de REVOKE em `:1110` cobre só `^_vectorizer_`. A função (`theodb_rs/src/bench_symqg.rs:48`) faz `std::fs::read` de path arbitrário → leitura de filesystem por role comum.
3. **Delete engolido (PII).** `theodb_rs/src/vectorizer.rs:460` — `let _ = Spi::run_with_args(...)` nos dois braços; delete falho é marcado `done` e o embedding permanece pesquisável.

Este discovery levanta **como extensões PG maduras estruturam** cada uma dessas três correções, para o plano do M144 nascer alinhado ao SOTA (Rule 9, não reinventar) e não introduzir workaround.

## Objective

Produzir um blueprint que responda, com citação a código real de peers e do repo, **como estruturar** (a) o script de upgrade 1.1.0→1.2.0, (b) o hardening/gating de `symqg_spike_bench`, e (c) a propagação de erro + dead-letter no delete do vectorizer — de forma que o plano do M144 seja implementável sem retrabalho e cada fix tenha um oráculo de teste.

**Success criteria do blueprint:** os 4 cantos populados; cada questão respondida com ≥1 citação `knowledge-base/references/{project}` verificada; ≥2 fontes independentes por técnica (R2 do `discover-phd-rigor`); recomendação explícita por fix.

## In-scope / Out-of-scope

| Reference project | In scope | Out of scope |
|---|---|---|
| `knowledge-base/references/pgvector/sql/` | cadeia de scripts `vector--X--Y.sql` (padrão incremental) | resto do repo |
| `knowledge-base/references/postgres/contrib/pg_trgm/` | `pg_trgm--1.*.sql` (chain incremental de contrib) | resto do contrib |
| `knowledge-base/references/postgres/src/backend/catalog/system_functions.sql` | REVOKE de funções fs-reading (`pg_read_file`, `lo_import`) | resto do backend |
| `knowledge-base/references/citus/src/backend/distributed/sql/` | padrão de chain multi-versão + REVOKE em massa | lógica distribuída |
| `knowledge-base/references/paradedb/` | error-handling/retry em bg worker (pgrx, mesma stack) | indexação BM25 específica |
| in-repo `theodb_rs/sql/`, `scripts/test-upgrade.sh`, `theodb_rs/src/vectorizer.rs` | prior art M137/M122 | — |

## ADRs (como investigar)

- **ADR-D1:** time-budget 3h total (fix pequeno, prior art forte). Priorizar pgvector+pg_trgm (chain) e postgres system_functions (REVOKE) — os dois com maior sinal.
- **ADR-D2:** o canto "Tools" foca no harness de upgrade (`scripts/test-upgrade.sh` in-repo + como peers testam upgrade), não em build genérico — é o oráculo do DoD bullet 1.
- **ADR-D3:** `pgvectorscale` deferido para o canto techniques (Rust/pgrx igual, mas sua chain é curta); paradedb cobre bg-worker error handling melhor.

## Research questions

### Corner: Techniques (3)

1. **Como pgvector e pg_trgm estruturam um script de upgrade incremental `X→Y`** — só o delta (novos CREATEs) ou full schema? Method: `Read knowledge-base/references/pgvector/sql/vector--0.7.4--0.8.0.sql` + `Read knowledge-base/references/postgres/contrib/pg_trgm/pg_trgm--1.5--1.6.sql`. Expected: confirmação de que o padrão é **delta-only** (só os objetos novos), contrastando com o full-schema atual do repo.
2. **Como o PG core aplica REVOKE em função que lê filesystem** — qual o padrão canônico de least-privilege? Method: `Read knowledge-base/references/postgres/src/backend/catalog/system_functions.sql` linhas ~688-710 (`lo_import`, `pg_read_file`). Expected: `REVOKE EXECUTE ON FUNCTION ... FROM public;` imediatamente após o CREATE — o padrão para o fix do symqg (ou a decisão de gate-out).
3. **Como paradedb (pgrx, mesma stack) propaga erro de SPI/bg-worker sem engolir** — retorna Result, ereport, ou marca job? Method: `Grep -rn "Spi::run\|Spi::connect\|\.ok()\|let _ =" knowledge-base/references/paradedb/pg_search/src` + Read do handler. Expected: padrão de propagação (não `let _ =`) para o fix do delete.

### Corner: Integration tests (2)

4. **Como um projeto testa que um `ALTER EXTENSION UPDATE` é total e não quebra o cluster** — existe harness? Method: `Read scripts/test-upgrade.sh` (in-repo) + `Grep -rn "ALTER EXTENSION.*UPDATE" knowledge-base/references/citus knowledge-base/references/paradedb`. Expected: o shape do harness de upgrade (instala versão antiga → upgrade → valida superfície) que o DoD bullet 1 precisa.
5. **Como testar negativamente que uma função é superuser-only** (role comum recebe erro de permissão)? Method: `Grep -rn "REVOKE\|has_function_privilege\|permission denied" knowledge-base/references/postgres/contrib/*/sql/` + `Read knowledge-base/references/postgres/contrib/citext/sql/create_index_acl.sql`. Expected: padrão de teste ACL negativo para o DoD bullet 2.

### Corner: Dependencies (1)

6. **A propagação de erro no delete precisa de nova dep, ou o mecanismo de dead-letter/retry do M122 já resolve?** Method: `Grep -rn "dead_letter\|purge_dead_letters\|retry\|attempts" theodb_rs/src/vectorizer.rs` (in-repo). Expected: confirmação de que o dead-letter existe (M122) e que o fix é só propagar o erro para o caminho de retry já presente — zero dep nova (parsimony rung 4).

### Corner: Tools (1)

7. **Qual o comando/CI que prova o upgrade no droplet** e como o M137 já o exercita? Method: `Read scripts/test-upgrade.sh` + `Grep -rn "test-upgrade\|ALTER EXTENSION" .github/workflows/`. Expected: o comando exato de reprodução para a evidência do DoD bullet 1.

## Coverage Matrix

| Q | Corner | Method | Answer shape |
|---|---|---|---|
| 1 | techniques | Read pgvector + pg_trgm upgrade sql | delta-only vs full-schema |
| 2 | techniques | Read postgres system_functions.sql | REVOKE-after-CREATE pattern |
| 3 | techniques | Grep+Read paradedb bg-worker | error propagation pattern |
| 4 | integration tests | Read test-upgrade.sh + grep citus/paradedb | upgrade harness shape |
| 5 | integration tests | Grep+Read postgres contrib ACL tests | negative ACL test pattern |
| 6 | dependencies | Grep vectorizer.rs dead-letter | zero-new-dep confirmation |
| 7 | tools | Read test-upgrade.sh + grep workflows | reproduction command |

100% das questões mapeadas a método. Nenhuma deferida.

## Halt-loop checkpoints (para /discover-execute)

- Uma sub-questão só é `done` quando a resposta cita um path real `knowledge-base/references/{...}` OU um path in-repo verificado.
- Q1 done quando o padrão delta-only estiver confirmado com ≥2 peers (pgvector + pg_trgm).
- Q2/Q3 done quando o padrão citar linha exata do peer.
- **EC-1 (Q3):** se paradedb não expuser um handler SPI-delete comparável ao vectorizer, cair para a fonte autoritativa in-repo (o caminho de retry/dead-letter do M122 em `theodb_rs/src/vectorizer.rs`) — o padrão Rust idiomático (`Result` propagado, `?`, `ereport`) independe do peer.
- **EC-2 (Q4):** se citus/paradedb não derem um harness de upgrade limpo no grep, Q4 é respondida pelo in-repo `scripts/test-upgrade.sh` (verificado) + o padrão delta-only de Q1 — não bloquear.

> v1.1 (2026-07-23): incorporados EC-1 e EC-2 do edge-case review (`.claude/knowledge-base/reviews/m144-remediation-edge-cases-2026-07-23.md`). EC-3 (geração do delta a partir do schema pgrx) é detalhe de Phase 5 — resolvido no `/to-plan`.

## Acceptance Criteria

- 7/7 questões respondidas com citação verificável.
- 4 cantos populados (≥1 questão cada).
- Blueprint recomenda explicitamente: (a) delta-only para o 1.1.0→1.2.0, (b) gate-out vs REVOKE para o symqg, (c) propagação-para-dead-letter para o delete.
- Nenhuma citação fabricada (`discover-confidence` hard cap).

## Global Definition of Done

Blueprint atinge ≥ SHIPPABLE_WITH_CAVEATS no `/discover-confidence` (golden rule `discover-blueprint-golden-rule.md`), com os 4 cantos e ADRs presentes.
