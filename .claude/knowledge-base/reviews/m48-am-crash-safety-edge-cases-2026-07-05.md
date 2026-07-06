# Discover Edge Case Review — m48-am-crash-safety

Date: 2026-07-05
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/m48-am-crash-safety-plan.md
Research questions analyzed: 10
Edge cases found: 7 (MUST FIX: 2, SHOULD TEST: 3, DOCUMENT: 2)

## MUST FIX

### EC-1: Q4 presume que existe um precedente COMPLETO de "generation pivot" no core — pode não existir; e Q4 depende de Q1 sem declarar
- **Affected question:** Q4
- **Family:** Interpretation / Dependency
- **Scenario:** o halt-loop procura em nbtree/GIN um padrão completo "escreve geração nova → pivota meta →
  recicla velha" e não encontra (o core atualiza metapages atomicamente — fast root, GIN pending head —
  mas NÃO faz shadow-generation de índice inteiro). O loop ou fabrica a resposta ou trava. Além disso, o
  desenho do pivot em UM registro depende do limite `MAX_GENERIC_XLOG_PAGES` (verificado:
  `generic_xlog.h:23` = `XLR_NORMAL_MAX_BLOCK_ID`) que é resposta da Q1 — dependência não declarada.
- **Impact:** resposta fabricada (hard cap de citação) OU Q4 BLOCKED desnecessariamente; desenho do #47
  sem a restrição de páginas-por-registro.
- **Suggested fix:** reformular Q4 para "qual é o PRIMITIVO de metapage-update atômico (fast root nbtree /
  GIN meta) e o que ele suporta" + declarar `depends: Q1` + resposta parcial explícita é válida ("não há
  precedente de shadow-generation completo; o primitivo single-record meta update é a âncora").

### EC-2: Q8 método subespecificado — os bindings do pgrx-pg-sys SÃO pré-gerados no registry, mas o plano não aponta o arquivo exato
- **Affected question:** Q8
- **Family:** Method / Citation
- **Scenario:** verificado nesta review: os bindings existem em
  `~/.cargo/registry/.../pgrx-pg-sys-0.16.1/src/include/pg17.rs` (grep confirmou `log_newpage_range`,
  `RelationGetNumberOfBlocksInFork`, `vacuum_delay_point` — 3 hits). Sem o path exato, o halt-loop pode
  gastar variantes de Fase A procurando bindings "gerados no build" (que não existem localmente —
  `$PGRX_HOME` ausente) e marcar BLOCKED um item que é respondível em 1 grep.
- **Impact:** BLOCKED falso em questão-chave de deps; atraso do budget.
- **Suggested fix:** fixar o método: `grep -n '<símbolo>' ~/.cargo/registry/src/*/pgrx-pg-sys-0.16.1/src/include/pg17.rs`
  (pré-verificado nesta review) + precedente de uso em `pgvectorscale/pgvectorscale/src/`.

## SHOULD TEST

### EC-3: Q9 exige o container rodando — pode não estar quando o halt-loop chegar lá
- **Affected question:** Q9
- **Suggested halt-loop checkpoint:** antes da Q9: `docker start theodb-m48-verify 2>/dev/null || docker run -d --name theodb-m48-verify -e POSTGRES_PASSWORD=theodb -p 55448:5432 theodb:m48-verify` + `pg_isready`; só então rodar `pg_config` dentro dele.

### EC-4: Q5 tem risco de scope-creep para o território do M55 (design de manutenção in-place)
- **Affected question:** Q5
- **Suggested halt-loop checkpoint:** Q5 termina na tabela de contraste (in-place vs rebuild-fold) + sanity-check do meta-pivot; QUALQUER desenho de manutenção in-place para o theodb é M55 — parar ali e anotar como seed.

### EC-5: Q6 Fase A pode vagar por 40+ TAP tests — o exemplar canônico já é conhecido
- **Affected question:** Q6
- **Suggested halt-loop checkpoint:** começar por `src/test/recovery/t/013_crash_restart.pl` (verificado nesta review: existe; `022_crash_temp_files.pl` como segundo exemplar) antes de qualquer grep exploratório.

## DOCUMENT

### EC-6: Versão do clone postgres
- **Accepted risk:** `postgres` REL_17_STABLE shallow (`--depth 1`, 2026-07-05, 179M) — casa com o
  container PG17 da imagem. Line numbers citados no blueprint valem para este snapshot; um `git pull`
  futuro pode deslocá-los. Aceito: o blueprint data o snapshot.

### EC-7: vectorchord fora de escopo apesar de ser AM Rust
- **Accepted risk:** AGPL (barrado por D1 para empréstimo de código). Leitura conceitual não agrega sobre
  pgvectorscale (mesma stack, licença permissiva). Aceito conforme Out-of-Scope do plano.

## Summary

| Question | Edges found | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|-------------|----------|-------------|----------|
| Q1 | 0 | 0 | 0 | 0 |
| Q2 | 0 | 0 | 0 | 0 |
| Q3 | 0 | 0 | 0 | 0 |
| Q4 | 1 | 1 | 0 | 0 |
| Q5 | 1 | 0 | 1 | 0 |
| Q6 | 1 | 0 | 1 | 0 |
| Q7 | 0 | 0 | 0 | 0 |
| Q8 | 1 | 1 | 0 | 0 |
| Q9 | 1 | 0 | 1 | 0 |
| Q10 | 0 | 0 | 0 | 0 |
| (plan-wide) | 2 | 0 | 0 | 2 |

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT (2 MUST FIX — ambos de 1 frase; absorver e bump v1.0 → v1.1)
