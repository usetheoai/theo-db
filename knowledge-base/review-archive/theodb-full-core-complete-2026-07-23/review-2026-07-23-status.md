# Review theodb-full — status honesto (2026-07-23)

**Verdict: BLOCKED / IN_PROGRESS** — o review de repo inteiro (10 pilares × 108 arquivos-fonte) NÃO
convergiu nesta sessão. Causa: o fan-out paralelo de agentes pesados (10 pillar workers, cada lendo
~12K linhas) estoura o **rate-limit da API** ("Server is temporarily limiting requests" — throttle
transiente, não limite de uso) repetidamente. Um review completo aqui exige execução **serial** (1–2
pilares por vez) ao longo de muitas iterações/sessões, ou um ambiente sem esse throttle.

## O que FOI corrigido (o pedido explícito do owner) — ✅ ENTREGUE

1. **Plugin `build_manifest.py` (raiz do bug):** adicionado `SKIP_ANY` + helper `_is_skipped` que ignora
   diretórios de build/vendor/cache (`target`, `node_modules`, `.venv`, `__pycache__`, `dist`, `build`,
   `vendor`, …) em **qualquer profundidade** (antes só o 1º componente do path era checado, então
   `benchmarks/*/target/` e `target/*.rmeta` rastreados por engano entravam no review e geravam 7 findings
   HIGH FALSOS de secret em metadata compilada do openssl). Testado 8/8 (incl. `src/build.rs` preservado,
   `docs/build/` skipado). Arquivo: `/home/paulo/.claude/skills/review-cycle/scripts/build_manifest.py`.
2. **Manifest reconstruído limpo:** 114 files (74 `.rs` fonte), 0 em `target/`, 0 findings fake, 0 blocking.

## Cobertura parcial obtida

- **Chief pass (6 arquivos, 10 pilares cada):** `migrate.rs`, `nl.rs`, `parquet.rs`, `hybrid.rs`,
  `am/page/mod.rs`, `vec/rabitq.rs` — todos maduros, bem-testados, injection-safe. Os 7 blocking fake
  do `target/` foram REFUTADOS pelo júri (7→0). **1 finding real (LOW, non-blocking):** `parquet.rs:263`
  `atomic_write_parquet` dá atomicidade (temp+rename) mas sem `fsync` antes do rename → export não é
  crash-durável (aceitável para o export re-executável `theodb.htap_refresh`).
- **Batch-núcleo (12 arquivos, 3 pilares completos):** `semantic_folders`, `license_provenance`,
  `system_design` escreveram JSONs; os outros 7 pilares foram cortados pelo rate-limit.

## Findings/pistas surgidas (precisam de verificação — não confirmados/juriados)

Dos JSONs completos + do progresso dos agentes cortados (evidência a validar num re-run serial):

- **[semantic_folders, MEDIUM×2 + LOW]** `am/` é god-folder (mistura columnar-TAM com index AMs); a
  família de quantizadores está partida (`vec/rabitq.rs`+`aq.rs` vs `pq`/`sbq`/`sq8` soltos no root);
  subsistema `graph` como prefixo `graph_*` no root plano em vez de pacote `graph/`; naming assimétrico
  `am/page/` (ivf+symqg) vs `am/hnsw_page/`. Organização/findability, sem impacto de correção.
- **[license_provenance]** proveniência exemplar/honesta nos 12 (limpo).
- **[security — PISTA, verificar]** `ann/hnsw.rs::from_bytes` valida counts+entry mas NÃO os índices de
  vizinhos. NOTA: `ann/hnsw.rs` é o índice IN-MEMORY (round-trip da própria serialização, path bench/spike);
  o índice de produção em disco é `am/hnsw_page/` (com `decode_meta` próprio). Provável LOW (defense-in-depth),
  não buraco no path shipado — precisa confirmar quem alimenta esse `from_bytes`.
- **[tests_deadcode_docs — PISTA]** `am/page/ivf.rs` (1210L) sem `mod tests` in-file, ao contrário dos
  irmãos `symqg.rs`/`columnar_codec.rs` — verificar cobertura por teste de integração antes de flagear.
- **[idiomaticity/code — PISTA]** `am/scan.rs` é outlier de error-handling vs o resto que usa
  `.unwrap_or_else(|e| err_input(...))`.
- **[maintainability — PISTA]** `am/page/ivf.rs` com duplicação de formato-por-versão — avaliar se é
  complexidade essencial por-versão (como o M145 ADR-2 provou para `main_index_pages`) ou extraível.

## Recomendação

Re-rodar o review **serialmente** (1–2 pilares por vez, ou o chief-orchestrator em passes pequenos) para
evitar o rate-limit, agora que o manifest está limpo. Priorizar as pistas acima (esp. a de security do
`hnsw`/`from_bytes` e a de tests do `ivf.rs`). O plugin está consertado para futuros runs limpos.
