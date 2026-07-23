---
slug: m144-remediation
milestone_id: M144
date: 2026-07-23
verdict: READY_TO_MERGE
agents: [council-rust-pgrx, council-security, council-index-storage, cross-validation]
---

# Review — M144 Remediação P0+P1

**Veredito consolidado: READY_TO_MERGE** — 0 BLOCKER, 0 HIGH. 4 agentes-conselho
adversariais (rust-pgrx, security, index-storage, cross-validation), cada um instruído a
verificar se as "notas honestas" dos ADRs escondiam fix incompleto.

## Severidade

| Sev | Origem | Finding | Ação |
|---|---|---|---|
| MEDIUM | security | Cobertura de redação incompleta (Basic/x-api-key/api-key Azure/cloud keys) — **pré-existente**, não regride M144 | Issue **#165** filada; não bloqueia |
| MEDIUM | cross-val | T2.1 sem `#[pg_test]` commitado (plano nomeava um teste inexistente) — PRE_COMMIT não é pg_test-ável | Plano reconciliado (smoke-only por design); commit `de…` |
| LOW | rust-pgrx | Comentário PII inline superdimensionava (`permanently searchable`) vs nota honesta do teste | **Corrigido** (soften para defense-in-depth) |
| LOW | index-storage | `\echo` do upgrade dizia `'1.1.0'` (deveria `'1.2.0'`) — cosmético (ALTER dispatch por filename) | **Corrigido** |
| LOW | cross-val | `backoff_saturates` era formula-lock, não exercitava o código | **Corrigido** (exercita `_vectorizer_mark_failed` attempts=60) |
| LOW | cross-val | DoD `cargo pgrx test exit 0` não atingido / Goal "cada RED" falso p/ T1.3 | Plano/DoD reconciliados honestamente |
| INFO | security | `let bytes: Vec<char>` mal-nomeado | **Corrigido** (→`chars`) |
| INFO | index-storage | empty-rows entries persistem no WRITE_STATES entre txns (pré-existente, inofensivo) | Registrado |

## Confirmações positivas (adversarial, com fonte primária)

- **rust-pgrx**: os 3 fixes (columnar `try_relation_open`, delete propagation, u32 guard) são **SOUND**. A afirmação de defense-in-depth do T1.3 foi **CONFIRMADA** contra a fonte pgrx 0.19 (`spi.rs:421-437`: DML error faz longjmp; `Err(SpiError(code))` só para status negativo).
- **security**: T1.2 REVOKE **correto e completo** (assinatura casa, `requires` ordena, upgrade cobre 1.1.0, parquet já REVOKEd, sem bypass via search_path/SECURITY DEFINER/default-priv). T2.2 desync fix **correto**; `to_ascii_lowercase` suficiente; cap 400 não vaza credencial coberta; afirmação honesta **acurada**.
- **index-storage**: T2.1 correto nos 4 pontos (semântica `try_relation_open`=NULL para self-drop confirmada em PG18 `relation.c`; descarte de pending limpo em ambos regimes; `WRITE_STATES.remove` fecha bug real de OID-reuse). T1.1 upgrade **crash-safe e idempotente** (AMs guardados por `IF NOT EXISTS`, amhandler `CREATE OR REPLACE` preserva OID, zero DROP destrutivo).
- **cross-validation**: os 7 fixes existem e batem com o CHANGELOG; as 3 notas honestas dos ADRs são **acuradas** (não desculpas); as 3 auto-correções de teste são reais nos commits; CHANGELOG **não** faz overclaim. "O autor está sendo honesto sobre as limitações."

## Gate

READY_TO_MERGE (≤2 HIGH, 0 BLOCKER — critério `cycle-review.md`). Ambos os MEDIUMs são
pré-existentes/documentais (issue #165 + reconciliação do plano), não bloqueiam. Todos os
LOW/INFO acionáveis corrigidos no próprio M144.
