---
slug: m148-flamegraph-spike
milestone_id: M148
created_at: 2026-07-24
goal: Produzir um flamegraph medido de uma query lenta do ClickBench sobre o theodb_columnar e emitir um veredito priorizando M149/M150/M151 conforme o frame de maior tempo próprio
---

# Plan: M148 — Flamegraph do scan colunar (spike measurement-first)

**Versão:** v1.1 (edge-case MUST FIX absorvido) · **Milestone:** M148 · **Tipo:** SPIKE (Phase 4 do `/theodb-evolution` — falsifiable spike,
termina com veredito) · **Data:** 2026-07-24

## Goal

Produzir um **flamegraph medido** de ≥1 query lenta do ClickBench (`columnar_customscan=False`, ~47s) sobre
1M linhas reais no `theodb_columnar`, e emitir um **veredito** dizendo qual das 3 técnicas (projection
pushdown / chunk-filter / vetorização) ataca o frame de maior **tempo próprio** — com o % que cada uma
endereçaria.

## Context

O gate ClickBench 1M pós-#190 (`docs/benchmarks/clickbench-1m-postfix-2026-07-24.md`) revelou que 36/43
queries rodam a ~47s pelo executor row-based. `memory/columnar-scan-bottleneck-hypothesis.md` aponta o
gargalo **por código** (decode das 105 colunas + re-materialização row-by-row), mas **não por profiling**.
Este spike fecha essa lacuna: mede *onde* o tempo vai, para M149/M150/M151 serem priorizados por evidência,
não hipótese. É o gate measurement-first (padrão M75/M83 — nenhuma aposta grande sem a medida).

## Baseline Context (deep review of current state)

### Files that will be touched

| Arquivo | LoC | Papel |
|---|---|---|
| `docs/benchmarks/m148-flamegraph-scan.md` (NEW) | — | O veredito + link para o SVG |
| `docs/benchmarks/m148-scan-flamegraph.svg` (NEW) | — | O flamegraph gerado |
| `benchmarks/profile_columnar_scan.sh` (NEW) | — | Harness reproduzível de profiling |

**Nenhum código de produção muda** — é medição. O deliverable é evidência, não uma feature.

### Current callers / dependents

Nenhum — os 3 arquivos são NOVOS (harness + doc + SVG) e não têm consumidores no código. O spike não
altera símbolo algum de `theodb_rs`; `grep -rn "profile_columnar_scan" .` retorna vazio até T1.1 criá-lo.

### Domain glossary

- **Tempo próprio (self time)** — tempo gasto numa função excluindo os filhos; é o que o flamegraph mede na
  largura de cada frame. É o que importa para priorizar: a função larga é o gargalo real.
- **`--call-graph dwarf`** — reconstrói a pilha via debug info em vez de frame pointers (que o release build
  omite). Necessário porque profilamos um build otimizado.
- **`pg_backend_pid()`** — o PID do backend da sessão, alvo do `perf record -p`.

### Architecture boundaries affected

Nenhuma — spike de medição, não toca a camada de storage nem a superfície SQL.

## Prior Art & Related Work

- **SOTA de profiling:** [Jan Nidzwetzki, "Analyzing PostgreSQL Performance Using Flame Graphs"](https://jnidzwetzki.github.io/2025/07/05/postgresql-flamegraph.html)
  (2025) — `perf record -g -F 111 -p <pid>`, `perf script | stackcollapse-perf.pl | flamegraph.pl`.
- **[The Rust Performance Book — Profiling](https://nnethercote.github.io/perf-book/profiling.html)** +
  **[flamegraph-rs](https://github.com/flamegraph-rs/flamegraph)** — release build precisa de `debuginfo=2`;
  inlining em `-O3` esconde frames, mitigado por `force-frame-pointers` + `--call-graph dwarf`.
- **Interno:** `memory/columnar-scan-bottleneck-hypothesis.md` (a hipótese que este spike testa),
  `docs/benchmarks/clickbench-1m-postfix-2026-07-24.md` (o baseline medido).

## Objective

Transformar a hipótese de gargalo (código) em medição (flamegraph), com veredito acionável.

## ADRs

### ADR-1 — Profilar release build com `debuginfo=2` + `--call-graph dwarf`, não debug build

**Decisão:** build de release (o que roda em produção) com `RUSTFLAGS="-C debuginfo=2 -C
force-frame-pointers=yes"`, e `perf record --call-graph dwarf`.

**Rationale:** o gargalo real é o do binário de produção (otimizado). Um debug build `-O0` teria perfil de
tempo diferente (sem inline, sem otimização de loop) — mediria a coisa errada. `debuginfo=2` dá os símbolos;
`dwarf` reconstrói a pilha sem depender de frame pointers. Fonte: Rust Performance Book + flamegraph-rs.

**Alternativa rejeitada — debug build `-O0`:** perfil não-representativo do que roda em produção.
**Alternativa rejeitada — release sem debuginfo:** flamegraph de endereços sem nome, inútil.

## Drawbacks & Risks

| # | Risco | Sev | Mitigação | Dono |
|---|---|---|---|---|
| R1 | O gargalo pode ser **I/O**, não CPU — o flamegraph de `perf` amostra CPU on-cpu | ALTA | Rodar 2ª medição com `perf stat` (cache-misses, ipc) e reportar CPU-bound vs I/O-bound explicitamente; se I/O-dominante, o veredito muda para "compressão/leitura", não pushdown | impl. |
| R2 | Inlining em `-O3` esconde o frame real | MÉDIA | `force-frame-pointers=yes` + `--call-graph dwarf`; se ainda opaco, comparar com um 2º build `-C opt-level=1` | impl. |
| R3 | `perf` pode não estar disponível no kernel do droplet (paravirt) | MÉDIA | Testar `perf record true` no setup; se falhar, fallback para `perf` do pacote genérico OU amostragem via `pg_stat_activity` + `EXPLAIN (ANALYZE, BUFFERS)` como sinal secundário | impl. |

## Unresolved Questions

- Q1 — Qual query lenta profilar? **Decisão:** a mais lenta com `cs=False` do gate (q33/q34, ~55s) — maior
  sinal. Se for atípica (regexp), profilar também uma de scan puro (q7/q8).
- Q2 — Um flamegraph basta ou precisa de vários? **Decisão:** 1 flamegraph da query dominante + `perf stat`
  para o eixo CPU-vs-I/O. Se ambíguo, um 2º da q7 (scan puro).

## Dependency Graph

```
T1 (harness de profiling) → T2 (rodar perf + gerar SVG) → T3 (veredito)
```

## Phase 1: Medição

### T1.1 — Harness de profiling reproduzível

#### Objective
`benchmarks/profile_columnar_scan.sh` que sobe o PG, carrega 1M linhas, dispara a query alvo e captura o
flamegraph do backend.

#### Why this step
**Ação:** script que pega `pg_backend_pid()`, roda `perf record --call-graph dwarf -F 111 -p <pid>` durante
a query, e gera o SVG. **Raciocínio:** reprodutibilidade — um flamegraph que ninguém sabe reproduzir não é
evidência (Regra 5, `public-copy.md`). O padrão vem do SOTA (Jan Nidzwetzki).

#### Files to edit
- `benchmarks/profile_columnar_scan.sh` (NEW)

#### TDD
- **RED/verificação:** o SVG gerado tem frames com nomes de função `theodb_rs::...` (não endereços crus).
- Se vier só endereços → o build não tem símbolos → falha, ajustar RUSTFLAGS.

#### Concurrency tests
`(none — single-threaded)`. O profiling é de um único backend; a query roda serial.

#### Acceptance Criteria
- `grep -c 'theodb\|columnar\|decode' docs/benchmarks/m148-scan-flamegraph.svg` retorna **> 0** (símbolos resolvidos, não endereços crus).
- `perf stat -e cycles,instructions,cache-misses` da query alvo é capturado em `docs/benchmarks/m148-perfstat.txt`.

#### Edge cases absorvidos (do review 2026-07-24)
- **EC-1 (MUST FIX):** o harness ABORTA se `perf script -i data.perf | wc -l` < 500 amostras — evita
  veredito sobre um flamegraph vazio (o análogo do harness vácuo do #190). Query alvo de ≥40s garante janela.
- **EC-3:** se um frame dominante vier sem símbolo (zstd dinâmico), o doc o nomeia honestamente
  ("descompressão (zstd, símbolo ausente)"), não omite o maior frame.

#### DoD
SVG gerado com símbolos + `perf stat` capturado + **≥ 500 amostras** no `perf.data`, tudo reproduzível pelo
script.

### T1.2 — Veredito priorizando M149/M150/M151

#### Objective
`docs/benchmarks/m148-flamegraph-scan.md` com o top-3 de frames por tempo próprio e qual técnica os ataca.

#### Why this step
**Ação:** ler o flamegraph, identificar os 3 frames mais largos (self time), mapear cada um a uma técnica.
**Raciocínio:** o M148 existe para *priorizar* — sem veredito acionável, o flamegraph é decoração.

#### Files to edit
- `docs/benchmarks/m148-flamegraph-scan.md` (NEW)

#### Concurrency tests
`(none — single-threaded)`. Análise de um flamegraph estático; não há execução concorrente nesta task.

#### Acceptance Criteria
- O doc lista os **3 frames de maior tempo próprio** com o % extraído do `data.folded` (soma de amostras do frame / total), não estimado.
- Cada um dos 3 frames é rotulado com **exatamente uma** das etiquetas `M149|M150|M151|nao-coberto`, e o doc soma o % que cada milestone endereçaria.
- O doc declara `CPU_BOUND` ou `IO_BOUND` citando o `ipc` e `cache-misses` do `perf stat` (ex.: ipc<0.5 + cache-miss alto = memory-bound).
- Se o frame dominante for `nao-coberto` (ex.: alloc/memcpy/WAL), o doc diz isso explicitamente e recomenda re-scoping de M149-M151 — honest-negative, não força um mapeamento.

#### Edge cases absorvidos (do review 2026-07-24)
- **EC-2 (SHOULD TEST):** profilar DUAS queries — a mais lenta (q33/q34) E uma de scan puro (q1/projeção
  simples). Se o frame dominante da q33 for `regexp_replace` (função do PG, não nosso scan), o veredito
  declara que essa query não é representativa e usa a de scan puro para priorizar. Obrigatório, não opcional.
- **EC-4 (DOCUMENT):** o % é do box de medição (c-8 DO); a ORDEM relativa dos frames é o entregável, não o
  valor absoluto.

#### DoD
Doc commitado com o veredito das DUAS queries; a ordem de M149/M150/M151 no ROADMAP é confirmada ou
reajustada com base nele.

## Coverage Matrix

| Requisito (DoD do M148 no ROADMAP) | Task |
|---|---|
| `perf record -g` num backend durante query lenta, droplet efêmero destruído | T1.1 |
| Flamegraph commitado com top-3 de frames por tempo próprio | T1.1, T1.2 |
| Veredito: qual técnica ataca o frame dominante + % | T1.2 |
| CPU-bound vs I/O-bound (R1) | T1.1, T1.2 |
| Harness aborta em medição vazia (EC-1) | T1.1 |
| Duas queries profiladas (EC-2) | T1.2 |
| CHANGELOG `[Unreleased]` | T1.2 |

Cobertura: **5/5 = 100%**.

## Global Definition of Done

- [ ] `m148-scan-flamegraph.svg` gerado com símbolos resolvidos.
- [ ] `perf stat` capturado (CPU-vs-I/O).
- [ ] `docs/benchmarks/m148-flamegraph-scan.md` com top-3 frames + veredito por técnica.
- [ ] `benchmarks/profile_columnar_scan.sh` reproduzível.
- [ ] CHANGELOG `[Unreleased]`.
- [ ] Droplet efêmero destruído (`doctl ... list --tag-name ephemeral-bench` = 0).

## Failure scenarios

`(none — no external I/O touched)`. O spike opera sobre um PG local no droplet; não há cliente de rede,
fila ou object store. O modo de falha relevante (perf indisponível no kernel) está em R3.

## Final Phase: Integration Validation

1. Rodar `benchmarks/profile_columnar_scan.sh` de ponta a ponta num droplet limpo.
2. Confirmar SVG com símbolos + veredito coerente com o `perf stat`.
3. Destruir o droplet; confirmar zero efêmeros.
