---
items: [B-029, B-013, B-027, B-039, B-022, B-016, B-023]
mode: review
date: 2026-08-13
verdict: pending
---

# Tier 1 — os portões existem, funcionam, e nunca veem o código antes do merge

## Corner 1 — Evidence

### O achado que nenhum dos cinco itens nomeia

Medido em 2026-08-13 lendo os `on:` dos dez workflows e o histórico de execuções:

**Todo gate roda apenas em `push` para `develop`/`main`.** O gatilho de `pull_request` foi removido em
2026-08-12 por decisão do owner, e a razão está escrita em todos os dez arquivos: *"o runner é único e serial,
e cada PR disparava a esteira inteira a cada push, com custo elevado"*.

O contrato de branching (`rules/git-safety.md § 1`) diz que **todo** trabalho nasce em `workspace`. A
composição das duas coisas é o defeito:

```
workspace  ──(nenhum gate roda aqui)──> PR #228 ──(nenhum gate roda no PR)──> merge ──> develop ──> gates rodam
```

**O primeiro momento em que um portão vê a mudança é depois de ela já estar integrada.**

### O número

Última execução de qualquer workflow em `workspace`: **2026-08-12T10:34**, sobre `48286921`.

| Desde então, em `workspace` | |
|---|---|
| commits | **73** |
| commits que tocam `theodb_rs/src/` | **13** |
| diff em `theodb_rs/` | **35 arquivos, +2.414 / −7.420 linhas** |
| execuções de gate sobre esse código | **0** |

Isto inclui o B-036, que alterou o caminho de build do índice vetorial em quatro call sites, e o B-046, que
mexeu no cliente de benchmark. Os 478 testes que eu rodei existem porque **eu** os rodei à mão, num contêiner
que montei à mão. Nada no sistema exigiu isso, e nada teria notado se eu não tivesse.

### Os cinco itens, medidos um a um — e três já estão fechados

| Item | Estado medido | Evidência |
|---|---|---|
| **B-029** | **ABERTO** | 10 invocações de 6 scripts ausentes: `ci.yml:243,444,520,657` (`smoke.sh`), `ci.yml:364,367,370` (migrate-*), `schema-drift-gate.yml:87,88` (`sql-surface.sh`), `cassert-sql-safety.yml:94` (`cassert-smoke.sh`) |
| **B-013** | **FECHADO** | `rust-suite.yml` roda `cargo pgrx test pg18` com `BASELINE=0`, reprova em regressão, e distingue "não emitiu resultado" de "reprovou" (`:149`). Verde em 2026-08-12 |
| **B-027** | **FECHADO** | O nome do contêiner virou `suite-${run_id}-${run_attempt}` — a colisão deixou de ser possível, em vez de ser remediada. Os 3 bullets do `dod` estão cobertos |
| **B-039** | **ABERTO** | `detectors/rust.py:123` invoca `cargo +nightly udeps` em `manifest_dir` **no host**, onde o pgrx nunca foi instalado |
| **B-022 / B-016 / B-023** | **não verificáveis sem rodar a suíte** | O baseline do `rust-suite.yml` é **0** desde 2026-08-12 (`440 passed; 0 failed`), o que sugere que já foram fechados — mas isso é inferência, e a suíte que eu rodei hoje deu `478 passed; 0 failed` |

**A hipótese mais dramática NÃO se sustenta.** Eu suspeitei que `schema-drift-gate` e `cassert-sql-safety`
estivessem passando **verdes invocando script inexistente** — o que seria muito pior que falhar. Medido:
as execuções verdes são de `2026-08-12T11:45` sobre `f3dd23b2`, e `8605677` (a remoção) **não está em
`develop` nem em `main`**. Elas passaram verdes porque os scripts ainda existiam. A quebra está armada, não
consumada.

### A decisão que o B-031 já tomou, e que muda 3 das 10 invocações

`theodb_rs/sql/` hoje contém **dois** itens: `schema_snapshot.sql` e `surface/`. A cadeia de upgrade foi
removida pelo B-031, com ADR. Então `migrate-doc-check.sh`, `migrate-smoke.sh` e `migrate-smoke-selftest.sh`
testam uma cadeia **que não existe mais**: restaurá-los seria restaurar um oráculo sem objeto.

`schema_snapshot.sql` sobreviveu — o insumo do oráculo sem o oráculo, exatamente como o B-029 registrou.

Os scripts são recuperáveis de `8605677^`: `smoke.sh` (204 linhas), `sql-surface.sh` (79),
`cassert-smoke.sh` (116).

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

Vale registrar, sem alegar medição: se o constraint fosse declarado, este item seria o candidato óbvio. O
runner é **único e serial**, e foi essa capacidade que forçou a remoção do gatilho de `pull_request`. Mas
isso é leitura, não medição de fluxo.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `.github/workflows/` | 3 arquivos com invocações mortas; o gatilho é decisão do owner e **não muda sem ele** |
| `scripts/` | volta a existir com **3** scripts (`smoke`, `sql-surface`, `cassert-smoke`), não com os 17 |
| Cadeia de upgrade | **não volta** — o B-031 decidiu por ADR, e reabrir seria desfazer decisão registrada |
| `theodb_rs/` | nenhuma mudança de produto neste item |
| Custo do runner | **é a restrição que criou o problema**: qualquer proposta que multiplique execuções contradiz a razão pela qual o gatilho foi removido |
| Todos os itens abertos | dependem disto: hoje cada entrega é validada por eu lembrar de rodar |

## Corner 4 — Verification

1. Nenhum workflow invoca caminho inexistente — provado por um verificador que **falha** contra a árvore de
   `8605677`, não apenas passa contra a de hoje.
2. Existe gate sobre `workspace` **sem multiplicar o custo do runner** — a restrição que criou o problema
   continua valendo.
3. O portão de drift de superfície SQL volta a comparar antes/depois consumindo `schema_snapshot.sql`.
4. Cada oráculo restaurado é provado **reprovando** um caso deliberadamente quebrado.
5. `cargo-udeps` reporta resultado, não `auditor_unavailable`.
6. B-013/B-027 são fechados **com a evidência que os fecha**, não por decreto.

## Reclassificação

`suggested_mode` era `bug` (B-029) e `evolve` (B-013). O modo real é **`review`**: a medição foi de leitura
de configuração e histórico, e o achado principal — a janela cega de `workspace` — não estava em nenhum item.
Ele nasce aqui e é registrado como item próprio.
