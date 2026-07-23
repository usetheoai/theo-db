# Implementation — M146 Hardening Remediation

**Plan:** `.claude/knowledge-base/plans/m146-hardening-remediation-plan.md` (SHIPPABLE 95.6)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m146-hardening-remediation-blueprint.md` (89)
**Baseline SHA:** `b948ea7` · **Commits:** `abfc3cd`, `1010a6c`, `fff9958`, `b7af091`, `979f116`, `998aea0`, `c38aa96`, `b08cb2b`
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
| **T2.2** `page/ivf.rs` testes | ✅ **entregue de outra forma, executando** | Ver § T2.2 revisado |
| **Review-fix** `#172` injeção em `recommend_ef` | ✅ **RED→GREEN discriminante** | Ver § Achados do review |

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

## T2.2 — revisado: entregue por outro mecanismo, e executando (commit `c38aa96`)

**Registro honesto da mudança de posição.** A versão anterior deste log declarou T2.2 "NÃO entregue", com o
argumento de que um `mod tests` in-file em `am/page/ivf.rs` produziria código que ninguém executa. O diagnóstico
estava certo; a **conclusão** estava errada. A resposta correta não era abandonar a cobertura — era tornar a
lógica executável. É o que o próprio projeto já fazia em `examples/resumable_check.rs` (M118) e no crate
`theodb_lexical` (M140.2); eu não olhei o precedente antes de declarar o gap.

O que mudou:

- As duas peças puras que mais importam foram extraídas para `am/page/ivf_codec.rs` (zero `pg_sys`): a
  codificação de labels de largura fixa (truncagem por overflow é limite documentado e load-bearing) e o
  cálculo de span com straddle de chunk (um off-by-one lê os bytes do vetor **errado** num rerank, sem falhar
  alto). `ivf.rs` **re-exporta** os itens — uma definição, nenhum gêmeo que derive.
- `read_record_at` passou a delegar a aritmética (e suas falhas tipadas) ao seam e ficou só com a I/O.
- O seam ganhou dois guards ausentes: registro maior que um chunk (o leitor costura no máximo 2 itens, então
  3+ retornaria buffer curto silenciosamente) e chunk/reclen zero.
- `examples/ivf_codec_check.rs` exercita tudo e **roda**: `IVF_CODEC_CHECK_OK`.

**Não-vacuidade provada por mutação** (o valor que um teste não executado nunca tem):

| Mutação | Resultado |
|---|---|
| clampar o ordinal em vez de falhar | panic em `ivf_codec_check.rs:94` |
| contagem de labels não satura em `LABEL_K` | panic: `count saturates at LABEL_K` |
| restaurado | `IVF_CODEC_CHECK_OK` |

**`with_soar_spill` (T2.3), agora com A/B medido** em vez de só "constrói e consulta": guard degenerado
(tabela vazia, λ>0) → build OK; λ=0 vs λ=1000 → 557056 B vs 688128 B (spill ativo); 50 linhas / 50 ids
distintos (dedupe por tid); recall@20 com 2 probes = 1.0. **Escopo honesto:** a reloption `soar_lambda` tem
mínimo 0 e rejeita −5 com "out of bounds", então o ramo `lambda < 0` do guard é inalcançável do SQL — não
afirmo tê-lo testado.

**Caminho de corrupção typed-error do IVF:** `corrupt_index.sh` foi parametrizado por AM (`AM=theodb_hnsw`
default, `AM=theodb_ivfflat`) em vez de duplicado. Sweep de 209 offsets sobre um índice IVF real com checksums
off → `deaths=0`; o caso detectado saiu como erro tipado nosso (`page … out of range (nblocks=20)`).

## Achados do review deste próprio milestone (commit `998aea0`)

O review encontrou seis pontos; todos corrigidos com evidência medida, e dois que são **decisão de design**
foram filados em vez de decididos sozinho (#173 gate de privilégio do `write_parquet`; #174 higiene do
`arrow_cache`).

- **#172 (HIGH, segurança)** — `theodb.recommend_ef` executava SQL arbitrário por **dois** eixos, ambos
  provados com o oráculo `1/0`: `qvec` entre aspas simples cruas e `col` entre aspas duplas cruas. Fix:
  `quote_literal` + validação fail-closed de identificador na fronteira, reusando o `valid_ident` existente.
  RED (os dois eixos): `ERROR: division by zero`. GREEN: `invalid input syntax for type vector` / `ERROR:
  22023 … must be a plain identifier`. Caminho honesto com índice HNSW real de 200 linhas → `ef=5`.
  **Nota de escopo:** `scan_stats` é `pub(crate)`; a superfície SQL é só `recommend_ef`, por onde ambos os
  eixos foram provados.
- **F1** — `max_level` não era validado: um valor corrompido faz a busca varrer ~4,3e9 níveis vazios **sem**
  `CHECK_FOR_INTERRUPTS` (query que não morre com Ctrl-C). Consequência operacional real e independente de panic.
- **F3** — taxonomia completada: 29 sites estruturais do `scan.rs` → `err_corrupt` (XX002), 3 de dim-mismatch
  → `err_input` (22023). Zero `pg_sys::error!` restantes. Sweep denso de 182 offsets: `deaths=0`, 13 detectados,
  todos erro tipado; SQLSTATE explícito `XX002` em `pg.rs:15` com backend `ALIVE 400`.
- **F2** — falha de `fsync`/`rename`/`try_clone` no export → `err_io` (58030) em vez de 22023. Medido: rename
  com destino=diretório → 58030 em `pg.rs:33`; diretório inexistente → 22023 em `pg.rs:44` (continua sendo erro
  de parâmetro, corretamente); caminho honesto → 709 B com magic `PAR1`.
- **F4** — comentário meu afirmando que `into_inner()` não escreve o footer estava **factualmente errado**
  (ele chama `write_metadata`). Corrigido.
- **F5** — o doc de `atomic_write_parquet` prometia mais do que o mecanismo entrega: se o `rename` tem sucesso
  e só o `fsync` do diretório falha, o arquivo **fica publicado** e a função retorna `Err`. Contrato exato
  documentado, notando que o `durable_rename` do PostgreSQL tem a mesma propriedade.
- **F6** — o handbook citava `scan_hnsw_structured` (removida neste milestone) e a classe de erro antiga.

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
