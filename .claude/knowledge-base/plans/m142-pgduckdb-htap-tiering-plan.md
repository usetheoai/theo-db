---
slug: m142-pgduckdb-htap-tiering
milestone_id: M142
created_at: 2026-07-22
goal: Tier out pg_duckdb from the default TheoDB image into an optional theodb-htap image, proven by both-image smokes + a measured size delta.
---

# Plan: M142 — Tier-out do pg_duckdb (imagem default enxuta + imagem opcional theodb-htap)

> **Version 1.0** — Remove o `pg_duckdb` (o único componente C++, +170 MB, `shared_preload_libraries`,
> `libcurl4`/httpfs) da imagem **default** do TheoDB e o move para uma imagem opcional `theodb-htap`
> (camada sobre a default, não fork). Motivação: o `theodb_columnar` own-code (M99–M115) já cobre o
> colunar in-DB transparente; o valor único restante do pg_duckdb é lakehouse de arquivos externos
> (aposta D2), fora do hot path AI-native e não dogfoodado. Resultado: imagem default menor e sem
> superfície C++/httpfs; a capacidade lakehouse continua opt-in via `theodb-htap`. Emenda o ADR-0020.

## Goal

> "Enable operadores do TheoDB to puxar uma imagem default sem pg_duckdb (mantendo o lakehouse via imagem opcional `theodb-htap`) so that a superfície default fica enxuta e sem o único componente C++/httpfs, measured by a suíte de validação M142 (smoke da imagem default + smoke da imagem htap no droplet) passar E o delta de tamanho default→htap ≥ 150 MB ser registrado em `docs/benchmarks/m142-pgduckdb-tiering.md`."

## Context

O M61 (ADR-0020) embarcou o `pg_duckdb` na imagem default como adoção permissiva (Regra 9) do pilar
columnar/HTAP. A justificativa mudou desde então: (1) o `theodb_columnar` own-code (M99–M115, ADR-0042)
entregou o colunar transparente **in-database** sobre tabelas PG vivas — exatamente onde o pg_duckdb
mediu **honest-negative** (0,63–0,89× sobre heap, ADR-0020 § Evidência); (2) o M64 (ADR-0023) provou que
não há plano único PG+DuckDB — o RAG/AI-native é 100% PostgreSQL, pg_duckdb fora do hot path; (3) o M97
(`docs/benchmarks/m97-htap-viability.md`) recomendou **DEFER** um pilar columnar novo (espaço permissivo
esgotado). O valor **único** restante do pg_duckdb é lakehouse de arquivos externos (Parquet/Iceberg/CSV,
aposta D2). O próprio ADR-0020 § Consequências deixou o tiering como follow-up "Unresolved". Este plano
resolve esse follow-up: default enxuta, htap opt-in.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `Dockerfile` | 108 | `68ae076` (2026-07-21) | Build multi-stage: theodb_rs + pg_duckdb → runtime | `CREATE EXTENSION theodb/theodb_rs CASCADE` no init NÃO pode quebrar sem pg_duckdb; theodb_rs+columnar intactos |
| `sql/85-theodb-htap.sql` | 193 | `5caf3ed` (2026-07-09) | Codegen plpgsql da superfície M62 (build de statements, não executa DuckDB) | Funções continuam `CREATE`-áveis sem pg_duckdb; assinaturas inalteradas; extensão `theodb` idêntica nas 2 imagens |
| `sql/theodb--1.4--1.5.sql` (NEW) | 0 | — | (a criar) delta de upgrade in-place que re-aplica as 2 funções guardadas (disciplina M137 — mudar o corpo exige caminho de upgrade p/ instalações não-greenfield) | byte-idêntico em intenção ao `sql/85`; `CREATE OR REPLACE` idempotente |
| `theodb.control` | 5 | — | metadados da extensão umbrella | bump `default_version` 1.4→1.5 (o delta acima) |
| `.github/workflows/ci.yml` | 465 | `24d89a3` (2026-07-21) | CI: builda a imagem default + smokes + bench | Jobs default existentes continuam verdes; sem regressão |
| `README.md` | 205 | `d9fcd20` (2026-07-22) | Descrição de capacidades | Honestidade de posicionamento (public-copy.md) |
| `packaging/Dockerfile.htap` (NEW) | 0 | — | (a criar) imagem htap = default + pg_duckdb | — |
| `sql/tests/htap_guard_test.sql` (NEW) | 0 | — | (a criar) regressão do guard fail-closed | — |
| `scripts/m142-tiering-validate.sh` (NEW) | 0 | — | (a criar) build+smoke das 2 imagens + delta | — |
| `docs/adr/0056-m142-pgduckdb-htap-tiering.md` (NEW) | 0 | — | (a criar) emenda ao ADR-0020 | — |
| `docs/benchmarks/m142-pgduckdb-tiering.md` (NEW) | 0 | — | (a criar) evidência do delta de tamanho + smokes | — |
| `CHANGELOG.md` | (existe) | — | Contrato público (Regra 6) | — |

### Current callers / dependents

- **Símbolo:** `theodb.olap_sql(regclass)`, `theodb.htap_refresh_sql(regclass)` em `sql/85-theodb-htap.sql`
- **Callers (produção):** nenhum caller interno no repo — é superfície pública para o cliente. Consumido via `scripts/m61-pgduckdb-smoke.sh`, `benchmarks/run_m62_htap.py` (testes/bench).
- **Callers (testes):** `benchmarks/tests/test_htap.py`, `scripts/m61-pgduckdb-smoke.sh`.
- **External (API pública consumida por outros repos):** não há dogfood em produção (M141 é o dogfood; independe). Blast radius baixo (pré-1.0).

### Domain glossary

- **tier-out** — remover um componente da imagem/pacote default e oferecê-lo numa imagem opcional separada.
- **codegen HTAP** — as funções `theodb.*_sql` que **constroem** (retornam TEXT) statements DuckDB que o *cliente* executa; elas mesmas nunca chamam `duckdb.query` (pg_duckdb proíbe DuckDB dentro de função).
- **fail-closed guard** — checagem em runtime que `RAISE`ia um erro tipado com próximo passo quando um pré-requisito (pg_duckdb) está ausente, em vez de produzir silenciosamente algo quebrado.
- **shared_preload_libraries** — GUC de boot; pg_duckdb exige estar nela (carregada no start do postmaster).

### Architecture boundaries affected

- **Packaging/distribution boundary** (Dockerfile) — separa a superfície default da superfície opcional htap. Direção: extração de um componente para uma camada opt-in.
- **Extension SQL surface** (`sql/85`) — a superfície `theodb` permanece idêntica nas duas imagens (não cruza fronteira de versão; o guard é runtime, não estrutura). Sem violação da cadeia de upgrade M137.

## Prior Art & Related Work

- **Internal blueprint:** `knowledge-base/discoveries/blueprints/m142-pgduckdb-htap-tiering-blueprint.md` (o mapa do build + a decisão guard-vs-concat).
- **ADRs internos:** ADR-0020 (embarcar pg_duckdb — a decisão emendada; tier-out era seu follow-up Unresolved), ADR-0021/0023 (M62/M64 — pg_duckdb fora do hot path), ADR-0042 (own-code columnar M99).
- **Evidência medida:** `docs/benchmarks/m97-htap-viability.md` (DEFER pilar columnar), `docs/benchmarks/m61-columnar-adoption.md` (honest-negative sobre heap).
- **Precedente de packaging:** `packaging/Dockerfile.regress` (Dockerfile secundário já usado no job `pg-regression` do ci.yml).
- **Rules:** `.claude/rules/parsimony-ladder.md` (anti-sunk-cost), `.claude/rules/error-handling.md` (§2 fail-fast typed), `.claude/rules/public-copy.md` (honestidade).

## Objective

- [ ] Guard fail-closed nas funções de codegen HTAP (`olap_sql`, `htap_refresh_sql`) — RAISE typed error quando pg_duckdb ausente, com regressão SQL.
- [ ] Imagem default builda sem pg_duckdb (estágio/COPY/preload/libcurl4/CREATE EXTENSION removidos).
- [ ] Imagem `theodb-htap` = default + camada pg_duckdb, com a superfície M62 funcionando e2e.
- [ ] Delta de tamanho default→htap ≥ 150 MB medido e registrado em `docs/benchmarks/`.
- [ ] ADR-0056 (emenda ao 0020) + README + CHANGELOG (Changed + BREAKING) atualizados.
- [ ] CI builda as duas imagens; smoke default assere pg_duckdb ausente; smoke htap assere presente+funcional.

## ADRs

### D1 — Guard fail-closed em runtime, NÃO concat condicional da extensão

- **Decision:** manter `sql/85-theodb-htap.sql` no concat da extensão `theodb` (idêntico nas duas imagens) e adicionar às funções de codegen que produzem statements pg_duckdb (`olap_sql`, `htap_refresh_sql`) um guard que `RAISE EXCEPTION` (tipado, com próximo passo: "pg_duckdb ausente — use a imagem theodb-htap") quando `to_regproc('duckdb.query') IS NULL`.
- **Rationale:** o codegen é plpgsql puro (não chama DuckDB internamente) → `CREATE`-a sem pg_duckdb. Guard runtime mantém a extensão **idêntica** nas duas imagens (sem version skew, cadeia de upgrade M137 intacta) e é fail-fast/typed (error-handling.md §2 — o próprio arquivo já usa o padrão para no-snapshot). KISS.
- **Alternatives considered:** (a) **Concat condicional** (dois `theodb--1.0.sql`) — REJEITADO: duas variantes da extensão = version skew + complexidade de upgrade. (b) **Deixar sem guard** — REJEITADO: no default o cliente receberia um statement que falha com erro obscuro (`duckdb.query does not exist`), violando honestidade/UX.
- **Consequences:** habilita default enxuta com HTAP fail-closed honesto; constringe: o guard adiciona uma checagem barata por chamada de codegen (irrelevante — codegen não é hot path).

### D2 — Imagem htap = `FROM <default>` + camada pg_duckdb (camada, não fork)

- **Decision:** `packaging/Dockerfile.htap` recebe `ARG THEODB_BASE` e faz `FROM ${THEODB_BASE}`, re-adicionando o estágio builder do pg_duckdb (via multi-stage no mesmo arquivo), o COPY dos artefatos, o `shared_preload_libraries`, o `libcurl4` e um initdb.d `01-create-pgduckdb.sql`.
- **Rationale:** a htap fica **em sync por construção** com a default (mitiga o risco de drift R2) e não duplica o build do theodb_rs. Precedente: `Dockerfile.regress` (secundário já no CI).
- **Alternatives considered:** (a) **Dockerfile.htap self-contained** (repetir todo o build) — REJEITADO: duplica o build theodb_rs, propenso a drift. (b) **Build ARG `WITH_HTAP` no Dockerfile único** — REJEITADO: mistura duas imagens num arquivo, dificulta o cache/CI e o `docker images` por-imagem.
- **Consequences:** habilita opt-in em sync; constringe: o CI precisa buildar a default primeiro e taguear antes da htap (ordem de jobs).

### D3 — Compat sinalizada como Changed + BREAKING (pré-1.0, sem forçar semver-major)

- **Decision:** CHANGELOG sob `### Changed` com marca `BREAKING:` (a imagem default perde pg_duckdb; use `theodb-htap`), não sob `### Removed`.
- **Rationale:** pré-1.0 (0.x); a capacidade **não** é removida (continua via htap) — é a *superfície default* que muda. `Changed + BREAKING:` é honesto (public-copy.md) sem inflar para semver-major. Decisão do owner (grill 2026-07-22).
- **Alternatives considered:** `### Removed` (→ bump MAJOR no /release) — REJEITADO pelo owner (a capacidade não some, só vira opt-in).
- **Consequences:** o /release deriva bump `minor` (Changed sem Removed); o aviso BREAKING fica visível nas release notes.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Quem puxa a default e chama a superfície M62 quebra | Medium | Guard fail-closed com mensagem clara ("use theodb-htap"); CHANGELOG Changed+BREAKING; blast radius baixo (pré-1.0, nenhum dogfood depende) | Eng |
| Drift entre as 2 imagens (htap não builda / desatualiza) | Medium | htap = `FROM default` (camada, não fork) → sync por construção; CI builda **as duas** + smoke htap no mesmo run | Eng |
| Build full das 2 imagens é caro (pgrx + C++ DuckDB) no CI | Low | Reusar cache do buildx entre jobs (padrão já no ci.yml); a htap reusa a default cacheada | Eng |
| Guard usa `to_regproc('duckdb.query')` — se pg_duckdb expuser a função sob outro nome, falso-negativo | Low | Checar também `pg_extension` extname='pg_duckdb'; o smoke htap prova o caminho positivo | Eng |

## Unresolved Questions

- Q1 — O `mkdir /var/lib/postgresql/htap` (Dockerfile L103-105) deve sair da default ou é inócuo o suficiente para ficar? (Resolução no plano: mover para a htap — só é usado quando pg_duckdb escreve Parquet; deixar na default é dir órfão inócuo mas confuso. Mover é mais limpo.)
- Q2 — O `ca-certificates` (L71) deve permanecer na default? (Sim — é dependência do theodb_rs AI surface, não do pg_duckdb; só `libcurl4` sai.)

## Dependency Graph

```
Phase 0 (guard fail-closed + regressão SQL)
   │
   ▼
Phase 1 (tier default Dockerfile) ──▶ Phase 2 (Dockerfile.htap FROM default)
   │                                        │
   ▼                                        ▼
Phase 3 (docs: ADR/README/CHANGELOG)   Phase 4 (CI wiring)
   │                                        │
   └────────────────┬───────────────────────┘
                    ▼
        Final Phase: Integration Validation (droplet: build 2 imagens, delta, smokes)
```

Phase 0 é bloqueador (o guard precisa existir antes de a default ser tierada, senão a default expõe HTAP quebrado). Phases 3 e 4 podem paralelizar após Phase 2. A Final Phase depende de tudo.

---

## Phase 0: Guard fail-closed nas funções de codegen HTAP

**Objective:** as funções `theodb.olap_sql` / `theodb.htap_refresh_sql` falham com erro tipado claro quando pg_duckdb está ausente, em vez de retornar um statement que quebra no cliente.

### T0.1 — Adicionar guard `pg_duckdb ausente → RAISE` + regressão SQL

#### Objective
Guardar as duas funções de codegen que produzem statements DuckDB para que, sem pg_duckdb, elas RAISE um erro tipado com o próximo passo, mantendo a extensão idêntica nas duas imagens.

#### Why this step (action + reasoning)

1. **What this step does** — adiciona no topo de `theodb.olap_sql` e `theodb.htap_refresh_sql` um `IF to_regproc('duckdb.query') IS NULL THEN RAISE EXCEPTION ... USING HINT = 'pull the theodb-htap image'`; cria `sql/tests/htap_guard_test.sql` que prova ambos os caminhos (ausente→erro; presente→statement).
2. **Why it is necessary now** — é o bloqueador do tier-out (D1): sem o guard, a imagem default (Phase 1) exporia funções que retornam statements quebrados. Fazer AGORA (antes de tierar) garante que a default nunca fica num estado "HTAP silenciosamente quebrado".

#### Evidence
`sql/85-theodb-htap.sql:38` ("NO function calls duckdb.query internally") + L137/L173 (o arquivo já usa `RAISE EXCEPTION` tipado para no-snapshot — o guard segue o mesmo padrão). Blueprint § "Decisão-chave".

#### Files to edit
```
sql/85-theodb-htap.sql — guard no topo de olap_sql + htap_refresh_sql (RAISE se pg_duckdb ausente)
sql/tests/htap_guard_test.sql (NEW) — RED: sem pg_duckdb, olap_sql/htap_refresh_sql RAISE; com pg_duckdb, retornam TEXT
```

#### Deep file dependency analysis
- `sql/85-theodb-htap.sql` (Baseline row: codegen plpgsql M62): adiciona 2 blocos IF-guard; assinaturas e retornos inalterados no caminho positivo. Downstream: `benchmarks/tests/test_htap.py`, `scripts/m61-pgduckdb-smoke.sh` rodam sobre a imagem htap (pg_duckdb presente) → caminho positivo intacto.

#### Deep Dives
- Invariante (Baseline): funções continuam `CREATE`-áveis sem pg_duckdb (o guard é runtime, dentro do corpo — não referencia `duckdb.*` em tempo de CREATE). `to_regproc('duckdb.query')` retorna NULL sem erro quando a função não existe.
- Edge case: pg_duckdb presente mas função sob outro nome → guard checa `to_regproc('duckdb.query')`; o smoke htap prova o positivo. Mitigação secundária (D3-risk): opcionalmente também `EXISTS(SELECT 1 FROM pg_extension WHERE extname='pg_duckdb')`.

#### Pseudo-code / Signatures
```sql
CREATE OR REPLACE FUNCTION theodb.olap_sql(p_rel regclass) RETURNS text ... AS $$
BEGIN
  IF to_regproc('duckdb.query') IS NULL THEN
    RAISE EXCEPTION 'theodb.olap_sql: pg_duckdb not installed'
      USING HINT = 'The HTAP/lakehouse surface requires pg_duckdb — pull the theodb-htap image.',
            ERRCODE = 'feature_not_supported';   -- 0A000, typed
  END IF;
  ... (corpo existente: resolve snapshot, RAISE no_data_found se ausente, build statement) ...
END $$;
```

#### Tasks
1. Adicionar o guard IF no topo de `theodb.olap_sql` (antes do lookup de snapshot).
2. Adicionar o mesmo guard no topo de `theodb.htap_refresh_sql`.
3. Criar `sql/tests/htap_guard_test.sql` (RED primeiro): asserção do erro sem pg_duckdb + asserção do TEXT com pg_duckdb.

#### TDD
```
RED:  htap_guard_absent — sem pg_duckdb, SELECT theodb.olap_sql('t'::regclass) RAISE ERRCODE 0A000 (feature_not_supported) com HINT sobre theodb-htap
RED:  htap_refresh_guard_absent — idem para htap_refresh_sql
GREEN: implementar os 2 guards
REFACTOR: extrair o guard num helper interno theodb._require_pgduckdb() se a duplicação de 2 incomodar (DRY — regra de 3 não atingida com 2, então provavelmente manter inline; decidir no GREEN)
VERIFY: psql -f sql/tests/htap_guard_test.sql (na imagem default: erro esperado; na htap: TEXT esperado)
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] Sem pg_duckdb, `olap_sql`/`htap_refresh_sql` RAISE ERRCODE `0A000` com HINT mencionando `theodb-htap`.
- [ ] Com pg_duckdb, `SELECT theodb.olap_sql('t'::regclass)` retorna um TEXT contendo a substring `duckdb.query` (assert `grep -q 'duckdb.query'` no smoke htap) = TRUE.
- [ ] `sql/85-theodb-htap.sql` continua ≤ 500 linhas.
- [ ] Extensão `theodb` cria sem erro numa imagem SEM pg_duckdb.

#### DoD
- [ ] `htap_guard_test.sql` passa nas duas imagens (erro na default, TEXT na htap).
- [ ] `CREATE EXTENSION theodb` verde sem pg_duckdb.

---

## Phase 1: Tier o Dockerfile default (remover pg_duckdb)

**Objective:** a imagem default builda e inicializa sem qualquer traço de pg_duckdb, mantendo theodb_rs + theodb_columnar intactos.

### T1.1 — Remover pg_duckdb do Dockerfile default

#### Objective
Remover o estágio `pgduckdb-builder`, o COPY dos artefatos, o append de `shared_preload_libraries`, o `libcurl4` e o `CREATE EXTENSION pg_duckdb` do initdb; mover o `mkdir /var/lib/postgresql/htap` para fora do default (Q1).

#### Why this step (action + reasoning)
1. **What this step does** — edita `Dockerfile` removendo as linhas 34–48 (estágio), 60–66 (COPY+preload), `libcurl4` da L71, L100 (CREATE EXTENSION), L103–105 (mkdir htap).
2. **Why it is necessary now** — é o núcleo do tier-out; depende da Phase 0 (guard) já estar no lugar para que a superfície HTAP na default seja fail-closed, não quebrada.

#### Evidence
`Dockerfile:34-48,60-66,71,100,103-105` (o mapa exato — blueprint § "Mapa do build atual"). `Dockerfile:98` confirma que `theodb.control` NÃO depende de pg_duckdb (init não quebra).

#### Files to edit
```
Dockerfile — remover estágio pgduckdb-builder + COPY + shared_preload + libcurl4 + CREATE EXTENSION pg_duckdb + mkdir htap
```

#### Deep file dependency analysis
- `Dockerfile` (Baseline row): remove ~20 linhas do caminho pg_duckdb; `CREATE EXTENSION theodb/theodb_rs CASCADE` (L95-97) permanece e não depende de pg_duckdb. Downstream: os jobs do ci.yml que buildam a default continuam (Phase 4 ajusta os smokes).

#### Deep Dives
- Invariante (Baseline): init não quebra — `CREATE EXTENSION IF NOT EXISTS theodb CASCADE` e `theodb_rs CASCADE` não puxam pg_duckdb. Manter `ca-certificates` no apt (dep do theodb_rs), remover só `libcurl4`.
- Edge case: se algum outro SQL referenciar `duckdb.*` em tempo de CREATE → falharia; confirmado que só `sql/85` toca DuckDB e agora é guardado (Phase 0).

#### Tasks
1. Remover o bloco do estágio `pgduckdb-builder` (L34-48).
2. Remover os COPY de `pg_duckdb*` (L60-63) e o append de `shared_preload_libraries` (L64-66).
3. Remover `libcurl4` da linha de apt install (manter `ca-certificates`).
4. Remover `CREATE EXTENSION IF NOT EXISTS pg_duckdb;` do heredoc initdb (L100).
5. Remover o `mkdir/chown /var/lib/postgresql/htap` (L103-105).
6. Atualizar o comentário-cabeçalho do Dockerfile (L2) para refletir "sem pg_duckdb (tier-out M142)".

#### TDD
```
RED:  (validação de imagem — na Final Phase) default_no_pgduckdb — pg_extension sem pg_duckdb; shared_preload sem ele
GREEN: as remoções acima
REFACTOR: None expected
VERIFY: docker build -t theodb:m142-default . && smoke (Final Phase)
```
> Nota: a prova de imagem é a Final Phase (build real no droplet); Phase 1 é a edição que a habilita.

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `docker build .` (default) conclui sem o estágio pgduckdb.
- [ ] Container default sobe; `SELECT count(*) FROM pg_extension WHERE extname='pg_duckdb'` = 0.
- [ ] `SHOW shared_preload_libraries` não contém pg_duckdb.
- [ ] `theodb_rs` + tipo `vector` + `theodb_columnar` funcionam (smoke vetor/AM/columnar verde).

#### DoD
- [ ] Imagem default builda e inicializa limpa, sem pg_duckdb, com theodb_rs+columnar intactos (provado na Final Phase).

---

## Phase 2: Imagem opcional theodb-htap (camada sobre a default)

**Objective:** `packaging/Dockerfile.htap` produz uma imagem = default + pg_duckdb, com a superfície M62 funcionando e2e.

### T2.1 — Criar packaging/Dockerfile.htap

#### Objective
Re-adicionar o pg_duckdb como camada sobre a imagem default: builder multi-stage + COPY + `shared_preload_libraries` + `libcurl4` + initdb `01-create-pgduckdb.sql` + o `mkdir htap`.

#### Why this step (action + reasoning)
1. **What this step does** — novo `packaging/Dockerfile.htap` com `ARG THEODB_BASE=theodb:local`, `FROM ${THEODB_BASE}` para o runtime, e um estágio `pgduckdb-builder` (idêntico ao que saiu do default) para os artefatos.
2. **Why it is necessary now** — preserva a capacidade D2 opt-in em sync com a default (D2 do plano); sem isso, o tier-out seria remoção, não tiering.

#### Evidence
`Dockerfile:34-48,60-66,100,103-105` (o material movido). `packaging/Dockerfile.regress:1-30` (precedente de Dockerfile secundário com `FROM` da imagem theodb).

#### Files to edit
```
packaging/Dockerfile.htap (NEW) — FROM ${THEODB_BASE} + estágio pgduckdb-builder + COPY + preload + libcurl4 + initdb pg_duckdb + mkdir htap
```

#### Deep file dependency analysis
- `packaging/Dockerfile.htap` (NEW): não altera a default; consome-a via `THEODB_BASE`. O initdb `01-create-pgduckdb.sql` roda DEPOIS do `00-create-theodb.sql` da base (ordem lexicográfica) → theodb já existe quando pg_duckdb é criado.

#### Deep Dives
- Invariante: `shared_preload_libraries` precisa estar no `postgresql.conf.sample` ANTES do initdb (que roda no primeiro boot do container) — o append no build da htap satisfaz isso (o build não roda initdb).
- Edge case: `THEODB_BASE` não passado → default `theodb:local`; o script de validação passa a tag real.

#### Pseudo-code / Signatures
```dockerfile
# packaging/Dockerfile.htap — theodb-htap = default + pg_duckdb (camada, não fork). M142/ADR-0056.
ARG BASE_IMAGE=postgres:18-bookworm
ARG THEODB_BASE=theodb:local
FROM ${BASE_IMAGE} AS pgduckdb-builder
# ... (idêntico ao estágio removido do Dockerfile default) ...
FROM ${THEODB_BASE}
ARG PG_MAJOR=18
COPY --from=pgduckdb-builder .../pg_duckdb* .../lib/
COPY --from=pgduckdb-builder .../pg_duckdb* .../extension/
RUN ... append shared_preload_libraries='pg_duckdb' ...
RUN apt-get ... libcurl4 ...
COPY <<'EOF' /docker-entrypoint-initdb.d/01-create-pgduckdb.sql
CREATE EXTENSION IF NOT EXISTS pg_duckdb;
EOF
RUN mkdir -p /var/lib/postgresql/htap && chown postgres:postgres /var/lib/postgresql/htap
```

#### Tasks
1. Criar `packaging/Dockerfile.htap` com o estágio builder + camada runtime FROM base default.
2. Adicionar o initdb `01-create-pgduckdb.sql` (roda após o 00 da base).
3. Re-adicionar `mkdir/chown /var/lib/postgresql/htap`.

#### TDD
```
RED:  (Final Phase) htap_has_pgduckdb — pg_extension COM pg_duckdb; shared_preload contém pg_duckdb
RED:  (Final Phase) htap_m62_e2e — theodb.htap_refresh_sql/olap_sql produzem e o cliente executa (COPY parquet + duckdb.query)
GREEN: o Dockerfile.htap acima
REFACTOR: None expected
VERIFY: docker build -f packaging/Dockerfile.htap --build-arg THEODB_BASE=theodb:m142-default -t theodb:m142-htap . && smoke htap
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `docker build -f packaging/Dockerfile.htap` conclui.
- [ ] Container htap: `pg_extension` contém pg_duckdb; `shared_preload_libraries` contém pg_duckdb.
- [ ] `theodb.htap_refresh_sql(t)` → COPY parquet executa; `theodb.olap_sql(t)` → duckdb.query executa (e2e, reusando `scripts/m61-pgduckdb-smoke.sh` como base).

#### DoD
- [ ] Imagem htap builda e a superfície M62 funciona e2e (provado na Final Phase).

---

## Phase 3: Docs — ADR-0056 (emenda ao 0020) + README + CHANGELOG

**Objective:** a decisão de tier-out é rastreável (ADR), o posicionamento é honesto (README), e a mudança de compat é comunicada (CHANGELOG Changed+BREAKING).

### T3.1 — ADR-0056 + README + CHANGELOG

#### Objective
Escrever ADR-0056 emendando o 0020; mover pg_duckdb/HTAP no README de "default" para "opcional (theodb-htap)"; CHANGELOG Changed com `BREAKING:`.

#### Why this step (action + reasoning)
1. **What this step does** — cria `docs/adr/0056-m142-pgduckdb-htap-tiering.md`; edita `README.md` (§ Capacidades); adiciona entradas no CHANGELOG.
2. **Why it is necessary now** — o ADR-0020 era LOCKED por owner; emendá-lo exige ADR + CHANGELOG (Golden Rule Change Protocol). O README precisa parar de listar pg_duckdb como capacidade default (public-copy.md).

#### Evidence
`docs/adr/0020-m61-embed-pgduckdb.md` (§ Consequências — o follow-up Unresolved). `README.md:75,123` (menções a pg_duckdb como default). `.claude/rules/public-copy.md` (§3 honestidade).

#### Files to edit
```
docs/adr/0056-m142-pgduckdb-htap-tiering.md (NEW) — decisão de tier-out, emenda ao 0020, alternativas, consequências
README.md — mover pg_duckdb/HTAP de default para "opcional via imagem theodb-htap"
CHANGELOG.md — Changed: "BREAKING: imagem default não inclui mais pg_duckdb; use theodb-htap p/ lakehouse"
```

#### Deep file dependency analysis
- `README.md:75,123` (Baseline row): 2 menções a pg_duckdb como parte do default → reescrever como opt-in.
- `CHANGELOG.md`: já tem a entrada Added do roadmap-feature; adicionar Changed+BREAKING (do tier-out em si).

#### Tasks
1. Escrever ADR-0056 (status Accepted, emenda ao 0020, D1/D2/D3 deste plano).
2. Editar README (§ Capacidades — pg_duckdb vira opcional; e o L75 do bloco de arquitetura).
3. CHANGELOG `### Changed`: `- BREAKING: a imagem default não inclui mais pg_duckdb (tier-out M142); o lakehouse de arquivos externos (Parquet/Iceberg/CSV) continua via imagem opcional theodb-htap (#M142)`.

#### TDD
```
RED:  N/A (docs) — validação via check_xrefs + review
GREEN: os docs acima
REFACTOR: None expected
VERIFY: python3 scripts/check_xrefs.py (se aplicável) + hooks/public-copy-lint.sh sobre README
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `test -f docs/adr/0056-m142-pgduckdb-htap-tiering.md` = 0 E `grep -c '0020' docs/adr/0056-*.md` ≥ 1 E a seção `## Alternativas` tem ≥ 1 item (`grep -c` ≥ 1).
- [ ] `grep -c pg_duckdb README.md` na seção de capacidades default = 0; a menção sobrevive só sob "opcional/theodb-htap" (`grep -A2 theodb-htap README.md` contém pg_duckdb).
- [ ] `grep -c 'BREAKING:' CHANGELOG.md` ≥ 1 dentro de `### Changed` do `[Unreleased]`.
- [ ] `hooks/public-copy-lint.sh` sem novos warnings no README.

#### DoD
- [ ] Docs consistentes; xrefs resolvem.

---

## Phase 4: CI — buildar as duas imagens + asserções

**Objective:** o CI builda a default e a htap, o smoke default assere pg_duckdb ausente, o smoke htap assere presente+funcional.

### T4.1 — Wire ci.yml para as duas imagens

#### Objective
Adicionar ao ci.yml: uma asserção "pg_duckdb ausente" no smoke da imagem default e um job/step que builda `Dockerfile.htap` (FROM a default cacheada) + roda o smoke htap.

#### Why this step (action + reasoning)
1. **What this step does** — edita `.github/workflows/ci.yml`: (a) no smoke default, assere ausência de pg_duckdb; (b) novo step/job builda a htap e roda `scripts/m61-pgduckdb-smoke.sh` sobre ela.
2. **Why it is necessary now** — sem o CI das duas imagens, a htap apodrece (risco R2). Fazer agora fecha o gate de regressão.

#### Evidence
`.github/workflows/ci.yml:34-67` (job image-and-bench + smoke), `:171-192` (pg-regression usa Dockerfile secundário — precedente). `scripts/m61-pgduckdb-smoke.sh` (smoke htap reusável).

#### Files to edit
```
.github/workflows/ci.yml — assert pg_duckdb ausente no smoke default; novo job htap-image (build Dockerfile.htap + smoke)
```

#### Deep file dependency analysis
- `ci.yml` (Baseline row): adiciona 1 asserção + 1 job. Reusa `docker/build-push-action@v6` e o cache buildx já configurados. O job htap depende (`needs`) do job que builda/taguea a default.

#### Tasks
1. No smoke default, adicionar asserção: `pg_extension` sem pg_duckdb (falha o job se presente).
2. Novo job `htap-image`: `needs: image-and-bench`; build `packaging/Dockerfile.htap` com `THEODB_BASE` = tag da default; roda `scripts/m61-pgduckdb-smoke.sh`.

#### TDD
```
RED:  (o próprio CI é a prova) — o job htap-image falha se a htap não buildar ou o smoke falhar
GREEN: os steps acima
REFACTOR: None expected
VERIFY: o run do CI no PR (ou local `act` se disponível; senão, o build local no droplet cobre a substância)
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] No smoke default, `SELECT count(*) FROM pg_extension WHERE extname='pg_duckdb'` = 0 (o step falha com exit≠0 se ≠ 0).
- [ ] Job `htap-image` conclui com exit 0 (build `packaging/Dockerfile.htap` + `bash scripts/m61-pgduckdb-smoke.sh` retornam 0).

#### DoD
- [ ] CI cobre as duas imagens.

### T4.2 — Script de validação das 2 imagens + doc de benchmark do delta

#### Objective
Criar `scripts/m142-tiering-validate.sh` (build+smoke das duas imagens + medição do delta de tamanho) e `docs/benchmarks/m142-pgduckdb-tiering.md` (o artefato de evidência do delta ≥ 150 MB).

#### Why this step (action + reasoning)
1. **What this step does** — cria o script que sobe as duas imagens, roda os smokes (default: pg_duckdb ausente + guard RAISE + theodb_rs/columnar; htap: pg_duckdb presente + M62 e2e) e calcula/registra `size(htap) - size(default)`; cria o doc de benchmark com os números medidos.
2. **Why it is necessary now** — é o instrumento que produz a evidência medida do Goal (delta ≥ 150 MB) — sem ele o milestone não teria a prova reproduzível exigida (`public-copy.md` — performance/tamanho é claim, não opinião).

#### Evidence
`Dockerfile` (as 2 imagens a comparar), `scripts/m61-pgduckdb-smoke.sh` (smoke htap reusável — Regra 9), `scripts/m140-4-lexical-robustness.sh` (padrão de script de validação com gates honestos).

#### Files to edit
```
scripts/m142-tiering-validate.sh (NEW) — build+smoke das 2 imagens + delta; falha se delta < 150 MB
docs/benchmarks/m142-pgduckdb-tiering.md (NEW) — os números medidos (docker images) + os resultados dos smokes
```

#### Deep file dependency analysis
- `scripts/m142-tiering-validate.sh` (NEW): orquestra `docker build` + `docker run` + `psql`; reusa `scripts/m61-pgduckdb-smoke.sh` para o caminho htap. Downstream: rodado na Final Phase e (opcionalmente) referenciado pelo CI.
- `docs/benchmarks/m142-pgduckdb-tiering.md` (NEW): consumido pelo review como evidência.

#### Deep Dives
- Invariante: o delta é medido de `docker images --format '{{.Size}}'` das duas tags reais (não estimado). O script falha (exit≠0) se o delta < 150 MB ou qualquer smoke falhar — gate honesto (não "deve funcionar").
- Edge case: build do pg_duckdb C++ demora; o script tem timeout generoso e loga cada etapa.

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `bash scripts/m142-tiering-validate.sh` imprime `M142_TIERING_OK` e sai 0 quando: default sem pg_duckdb, htap com pg_duckdb+M62 e2e, e delta ≥ 150 MB.
- [ ] `docs/benchmarks/m142-pgduckdb-tiering.md` contém os dois tamanhos medidos (`docker images`) e o delta em MB.

#### DoD
- [ ] Script sai 0 no droplet; doc de benchmark com números reais.

---

## Coverage Matrix

| # | Gap / Requirement (DoD do M142) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Default builda sem pg_duckdb (estágio/COPY/preload/libcurl4/CREATE EXTENSION removidos) | T1.1 | Remoções no Dockerfile |
| 2 | Smoke default: pg_duckdb ausente + theodb_rs/columnar intactos | T1.1, T4.1, Final | Asserções no smoke |
| 3 | Dockerfile.htap = default + pg_duckdb, M62 e2e | T2.1, Final | Nova imagem + smoke htap |
| 4 | sql/85 + CREATE EXTENSION pg_duckdb condicionais à htap (via guard + initdb da htap) | T0.1, T1.1, T2.1 | Guard runtime (D1) + initdb só na htap |
| 5 | Delta ≥ 150 MB medido em docs/benchmarks/ | T4.2 | Script mede `docker images` das 2 imagens + escreve o doc |
| 6 | ADR emendando 0020 + README + CHANGELOG (Changed+BREAKING) | T3.1 | Docs |
| 7 | CI builda as duas imagens | T4.1 | Job htap-image + asserção default |

**Coverage: 7/7 gaps covered (100%)**

## Global Definition of Done

- [ ] Todas as fases completas.
- [ ] `sql/tests/htap_guard_test.sql` verde nas duas imagens (erro na default, TEXT na htap).
- [ ] `docker build .` (default) + `docker build -f packaging/Dockerfile.htap` (htap) ambos concluem no droplet.
- [ ] Smoke default: pg_duckdb ausente, theodb_rs+`vector`+theodb_columnar verdes.
- [ ] Smoke htap: pg_duckdb presente + M62 (`htap_refresh_sql`/`olap_sql`) e2e.
- [ ] Delta default→htap ≥ 150 MB registrado em `docs/benchmarks/m142-pgduckdb-tiering.md` (com `docker images`).
- [ ] ADR-0056 + README + CHANGELOG (Changed+BREAKING) atualizados.
- [ ] `.github/workflows/ci.yml` builda as duas imagens.
- [ ] File-size budget respeitado (Dockerfile.htap e sql/85 ≤ 500 linhas).
- [ ] CHANGELOG.md atualizado sob `[Unreleased]`.
- [ ] Backward compat: a superfície `theodb` é idêntica nas duas imagens (sem version skew); a única mudança de compat é a imagem default não ter mais pg_duckdb (documentada BREAKING).
- [ ] **Plan archived** — após `/review` READY_TO_MERGE + PR merged, mover para `knowledge-base/plans/completed/`.

## Failure scenarios

O plano toca I/O externo apenas indiretamente (o pg_duckdb httpfs), e apenas na imagem **htap** — o caminho default não tem I/O externo novo. A superfície M62 já existia (M61/M62); este plano não altera seu comportamento de I/O, só a empacota como opt-in.

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `pg_duckdb` (ausente na default) | função DuckDB não existe | rodar `theodb.olap_sql` na imagem default (sem pg_duckdb) | guard RAISE ERRCODE 0A000 com HINT "use theodb-htap" (T0.1) — nunca statement quebrado |
| `pg_duckdb` (presente na htap) | build da htap sem o `.so` | smoke htap assere `pg_extension` contém pg_duckdb | falha o job se ausente (T4.1) |

## Final Phase: Integration Validation (MANDATORY)

> Roda no **droplet e2e-runner** (165.227.121.20, Docker 29.4.1, 110 GB livres) — o único caminho para provar imagens reais (o padrão M99/M135: validar o binário/imagem que ships).

### Execution

```bash
# no droplet, no checkout do branch M142:
docker build -t theodb:m142-default .                                   # imagem default (sem pg_duckdb)
docker build -f packaging/Dockerfile.htap \
  --build-arg THEODB_BASE=theodb:m142-default -t theodb:m142-htap .      # imagem htap (camada)
docker images theodb:m142-default theodb:m142-htap                       # tamanhos → delta
bash scripts/m142-tiering-validate.sh                                    # sobe as 2, roda os smokes + guard + delta≥150MB
```

`scripts/m142-tiering-validate.sh` (NEW) faz:
1. Sobe container default → assere pg_extension sem pg_duckdb, shared_preload sem ele, theodb_rs/`vector`/theodb_columnar verdes, e `theodb.olap_sql` RAISE (guard).
2. Sobe container htap → assere pg_duckdb presente + `htap_refresh_sql`/`olap_sql` e2e (reusa `m61-pgduckdb-smoke.sh`).
3. Calcula o delta `size(htap) - size(default)` e falha se < 150 MB; escreve os números em `docs/benchmarks/m142-pgduckdb-tiering.md`.

### Acceptance Criteria

- [ ] `docker build` das duas tags (`theodb:m142-default`, `theodb:m142-htap`) retornam exit 0 no droplet.
- [ ] Smoke default: `pg_extension` count(pg_duckdb)=0 E smoke vetor/AM/columnar exit 0 E `theodb.olap_sql('t')` retorna SQLSTATE `0A000`.
- [ ] Smoke htap: `pg_extension` count(pg_duckdb)=1 E `theodb.olap_sql('t')`+COPY parquet executam com exit 0.
- [ ] Delta ≥ 150 MB medido e escrito em `docs/benchmarks/m142-pgduckdb-tiering.md` (`M142_TIERING_OK`).
- [ ] `sql/tests/htap_guard_test.sql` verde nas duas imagens.

### If Validation Fails

1. Identificar se a falha é do tier-out ou pré-existente (ex.: build do pg_duckdb C++).
2. Corrigir tudo causado por este plano antes de declarar completo.
3. Re-rodar a cadeia.
4. Issues pré-existentes são logados mas não bloqueiam (documentar no PR).
