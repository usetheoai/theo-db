# Deps-Audit — m48-am-crash-safety (plan-bound)

Date: 2026-07-05 · Verdict: **PASS**

- Plano declara `## Dependencies` completo: **zero dependências novas** (pgrx =0.16.1, criterion 0.5.1
  dev-only, harness Python — todas existentes e pinadas; coluna Rule 9 preenchida).
- `cargo audit` (theodb_rs): **0 erros/CVE**; 2 warnings allowlisted pré-existentes
  (RUSTSEC-2024-0436 `paste`, RUSTSEC-2021-0127 `serde_cbor` — advisories *unmaintained*, transitivos
  do pgrx pinado; não são CVEs; sunset gerido no allowlist do audit).
- Superfície nova = símbolos FFI já linkados pelo pgrx pinado (Blueprint §Q8) — nada novo a escanear.

Hard caps do golden rule: #1 golden rule presente ✓; #2 allowlist parse ✓; #3 sem CRITICAL/HIGH ✓;
#4 seção Dependencies presente/completa ✓.
