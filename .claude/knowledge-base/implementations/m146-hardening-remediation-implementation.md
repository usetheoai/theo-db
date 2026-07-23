# Implementation — M146 Hardening Remediation

**Plan:** `.claude/knowledge-base/plans/m146-hardening-remediation-plan.md` (SHIPPABLE 95.6)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m146-hardening-remediation-blueprint.md` (89)
**Baseline SHA:** `b948ea7` · **Commits:** `abfc3cd`, `1010a6c`, `fff9958`
**Substrato de validação:** droplet PG 18.4 (pgrx 0.19), extensão instalada via `cargo pgrx install`, testes SQL como `pgtest`.

## Restrição de substrato descoberta durante a execução (Regra 3)

A tier de **teste unitário não executa neste ambiente** — nem `cargo test`, nem `cargo pgrx test`. Ambos falham
ao linkar o target `lib test` com símbolos do backend indefinidos (`PG_exception_stack`, `errstart`, `errcode`,
`errmsg`, `errhint`, `errfinish`, `do_ereport`), porque a extensão é carregada *dentro* do postgres.
**Não é causado por este milestone:** o crate já tinha 69 `#[test]` e 326 `#[pg_test]` pré-existentes nessa
condição. Consequência: **todo RED→GREEN deste milestone foi medido no nível SQL/harness**, e nenhum DoD foi
marcado com base em teste não executado.

## Status por task — o que foi PROVADO vs o que foi implementado

| Task | Status | Evidência medida |
|---|---|---|
| **T1.2** `graph.rs` regclass | ✅ **RED→GREEN discriminante** | RED: `graph_build('(SELECT src,dst FROM g2 WHERE 1/0=1) x',…)` → `ERROR: division by zero` (só a execução da subquery injetada produz isso). GREEN: mesmo payload → `ERROR: invalid name syntax` (rejeitado antes de montar SQL). Controle honesto = 3 arestas; `s1.edges` schema-qualificado = 4 arestas (prova que `%I` seria errado) |
| **T1.3** `parquet.rs` durabilidade | ✅ **verificado** | Header e footer `PAR1` presentes (`5041 5231`), 1000 linhas, path relativo simples funciona (EC-2), zero temp órfão. **A primeira implementação foi REPROVADA pelo teste** (`finish()`+`into_inner()` → `SerializedFileWriter already finished`) e corrigida para `try_clone` + `close()` original |
| **T2.1** `err_corrupt` XX002 | ✅ **medido** | `ERROR: XX002: theodb am scan: theodb am: page 4243969085 out of range (nblocks=15)` / `LOCATION: theodb, pg.rs:15`; backend vivo. Contraste: corrupção no cabeçalho de página → PG responde XX001; no nosso leitor → XX002 |
| **T2.4** dead code + doc-drift | ✅ **aceite mecânico** | `grep scan_hnsw_structured` = 0 ocorrências. Doc do writer v6/SQ8 **movida** (não deletada) para `write_ivf_aq_split_sq8`, que estava sem doc. Pós-build: HNSW top-k=5, IVF top-k=5 |
| **T3.1** harness de corrupção | ✅ **entregue e executado** | `isolation/corrupt_index.sh` — 44 corrupções × 2 configurações, `deaths=0` em ambas |
| **T2.3** `with_soar_spill` | ✅ **validado via SQL** | Índice `WITH (lists=8, soar_lambda=1000)` constrói e consulta: top-k=10, idêntico ao baseline sem soar. Já existia `#[pg_test] ambuild_ivf_soar_spill_scans_high_recall_no_dupes` (escrito, não executável aqui) |
| **T1.1** `hnsw.rs` validação de vizinho | ⚠️ **implementado, SEM RED→GREEN discriminante** | Ver abaixo — reachability medida |
| **T2.2** `page/ivf.rs` testes in-file | ⚠️ **NÃO entregue** | Ver abaixo — motivo medido |

## T1.1 — correção de uma afirmação minha, com medição

Afirmei antes de medir que a validação faltante era "mais que defense-in-depth: um panic atravessando FFI
alcançável a partir de índice corrompido em disco". **A medição desmente isso.** 44 tentativas de corrupção
byte-level num índice HNSW real, em duas configurações:

- **`data_checksums=on`** (default do PG 18): `pg_page_gate=20`, `ours=0` — a verificação de página do
  PostgreSQL rejeita **toda** corrupção antes de o nosso código desserializar.
- **`data_checksums=off`**: `ours=4`, `pg_page_gate=0` — a corrupção alcança nosso código, e a validação de
  **cadeia de páginas que já existia** no leitor de blob a captura com erro tipado.
- **`deaths=0` nas duas.** Nenhuma corrupção derrubou o backend.

Ou seja: o `from_bytes` está protegido por duas camadas anteriores, e o gatilho residual seria um defeito no
nosso próprio writer ou corrupção de memória — não corrupção de disco. O fix entrou assim mesmo (fecha uma
invariante que o comentário do próprio arquivo já declarava, custo O(1) por vizinho sobre bytes já lidos), mas
**está documentado no código com essa reachability**, sem inflar a alegação. O harness fica como guarda de
regressão permanente da propriedade "nenhuma corrupção mata o backend".

## T2.2 — não entregue, com motivo

Os testes in-file de `am/page/ivf.rs` (LABEL_K truncation, straddle de chunk em `read_record_at`, paths de erro
tipado) exigem a tier unitária, que **não executa neste substrato**. Escrevê-los aqui produziria código que
ninguém executa — somando-se aos 395 testes já nessa condição — e marcá-los como DoD cumprido seria falso.
O codec **é** exercitado ponta-a-ponta pelos testes SQL (build + scan de índice IVF validados pós-build).
Fica como gap conhecido: reabrir quando a tier unitária linkar (ou num runner que a execute).

## Deps

`cargo audit` acusou `RUSTSEC-2026-0204` (`crossbeam-epoch 0.9.18`, transitiva via `crossbeam-utils`, sem CVSS,
gatilho = `fmt::Display` sobre `Atomic`/`Shared` — zero uso direto no nosso código). Resolvida com bump de
**lockfile apenas** para 0.9.20; `Cargo.toml` intocado, então o "zero dependência nova" do D2 se mantém.
`cargo audit` pós-bump: zero vulnerabilidades.

## Gates do plano

- **Zero dependência nova** ✓ (o fsync usa só `std`; errno definido localmente para não puxar `libc`)
- **Zero mudança de superfície SQL** ✓ (mesmas assinaturas `pg_extern`)
- **CHANGELOG `[Unreleased]`** ✓ (Added/Changed/Fixed/Security)
- **Commits sem trailer `Co-Authored-By`** ✓
