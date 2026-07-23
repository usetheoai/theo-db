# Deps Audit: m144-remediation

**Date:** 2026-07-23
**Mode:** plan-bound:m144-remediation
**Verdict:** PASS_WITH_CAVEATS
**Hard caps triggered:** [] (nenhum — o advisory é transitivo pré-existente, não dep declarada do M144)

## Summary
- Ecosystems detected: rust (`theodb_rs/Cargo.toml` + `Cargo.lock`)
- Deps novas introduzidas pelo M144: **0** (única mudança em Cargo.toml = feature `spike-symqg`, gateia código existente)
- Vulnerabilities: 0 CRITICAL, 0 HIGH, 0 MEDIUM em dep **declarada**; 1 advisory transitivo (severidade prática baixa) + 1 warning unmaintained
- Auditor coverage: { cargo-audit 0.22.1: ran, osv-scanner 1.9.2: available }

## Findings (transitivos / pré-existentes — NÃO introduzidos pelo M144)

### RUSTSEC-2026-0204 — `crossbeam-epoch 0.9.18` (transitivo)
- **Título:** Invalid pointer dereference in `fmt::Pointer` impl for `Atomic`/`Shared` when the underlying pointer is invalid.
- **Fixed in:** ≥ 0.9.20
- **Path:** `crossbeam-epoch` → `crossbeam-deque` → `rayon-core` → `rayon` → { `tantivy 0.26.1` (atrás de `spike-lexical`, **non-default**), `criterion 0.5.1` (**dev/bench only**) }
- **Severidade prática:** baixa — é um deref inválido no *debug-formatting* (`{:p}`) de um ponteiro já inválido; não é exploit remoto e não está no caminho de runtime dos fixes do M144. Nenhum dos consumidores (`tantivy` non-default, `criterion` dev) está no `.so` default shipado.
- **Diff suggestion (ortogonal ao M144, opcional):**
  ```
  cd theodb_rs && cargo update -p crossbeam-epoch --precise 0.9.20
  ```
  Bump de lockfile puro (transitivo, sem mudança de manifest). Recomendado como higiene, **não bloqueia M144**.

### RUSTSEC-2021-0127 — `serde_cbor 0.11.2` (warning, unmaintained)
- Via `pgrx 0.19.0` → `theodb_rs`/`pgrx-tests`. Warning "unmaintained", não vulnerabilidade. Fora do controle do repo (dep do framework pgrx). Sem ação no M144.

## Plan validation

| Plan dep | Section | Manifest match | Audit clean? | Rule 9 OK? | Verdict |
|---|---|---|---|---|---|
| `pgrx 0.19.0` | Existing | yes | yes (só warning transitivo serde_cbor via pgrx) | n/a | OK |
| `datafusion`/`arrow` | Existing | yes (já no tree, M143) | yes | n/a | OK |
| (nenhuma NEW) | New | n/a | n/a | n/a (0 deps novas) | OK |

## Recommended next steps

1. (Opcional, higiene, ortogonal) `cargo update -p crossbeam-epoch --precise 0.9.20` — bump de lockfile transitivo; pode entrar no M144 ou num chore separado.
2. Prosseguir com `/plan-confidence m144-remediation` — verdict PASS_WITH_CAVEATS não introduz hard cap (nenhuma dep declarada do M144 tem CVE HIGH/CRITICAL).

## Honest note

O M144 é uma remediação de código próprio sob TDD, sem superfície de dependência nova. O único achado com severidade é um advisory transitivo **que já existia antes do M144** e vive atrás de um feature non-default (`spike-lexical`) + uma dep dev (`criterion`) — nenhum no caminho default shipado. Registrado por honestidade (Regra 3), não como bloqueio.
