---
slug: theodb-rs-upgrade-chain
milestone_id: M137
date: 2026-07-21
reviewers: council-index-storage
---

# Review — M137 / cadeia de upgrade do `theodb_rs`

**Verdict:** READY_TO_MERGE

## Achados e desfecho

| Sev | Achado | Estado |
|---|---|---|
| HIGH | **F1** — shell type e definição completa compartilhavam o mesmo predicado de guarda; num catálogo sem o tipo, o shell era criado e a definição **nunca aplicava** (tipo shell para sempre, sem erro). Predicado também não qualificava namespace, então um `vector` de outra extensão satisfaria os dois. | **FECHADO** — guards diferem (`typisdefined`) + namespace qualificado; **provado empiricamente** sobre shell type real (antigo→`f`, novo→`t`) |
| HIGH | **F5** — nada impedia `1.1.0` de rotular múltiplos catálogos: o pgrx regenera o script base do HEAD a cada build enquanto o de upgrade é snapshot congelado | **FECHADO** — `.github/workflows/schema-drift-gate.yml` |
| HIGH | **F4** — `test-upgrade.sh` não existia; provas manuais e irreproduzíveis num artefato que já registrara duas leituras falsas | **FECHADO** — harness roda os 4 cenários e **aborta se o envelhecimento não remover nada** (fecha o pass vacuoso) |
| MEDIUM | **F2** — `CREATE TABLE IF NOT EXISTS` não converge drift de coluna | **ACEITO** — revisor mediu: nenhum caso vivo (tabelas nasceram inteiras). Convenção registrada: coluna nova ships como `ALTER TABLE ADD COLUMN IF NOT EXISTS` |
| MEDIUM | **F3** — `EXCEPTION` engole "mesmo objeto, definição diferente" em opclass/cast | **ACEITO** — opclasses byte-idênticas desde v0.60.0; morde na primeira mudança de membro |
| LOW | **F8** — limites obsoletos no artefato contradiziam §5/§6 | **CORRIGIDO** |

Confirmado correto pelo revisor: semântica transacional limpa (nenhum `CONCURRENTLY`/`VACUUM`/`ALTER SYSTEM`);
`CREATE OR REPLACE` preservando ACL como escolha certa, com a medição 87 == 87 como prova; `DROP FUNCTION IF
EXISTS` de ex-membro é legal (precedente pgvector); o corpo das funções C não precisa entrar no catálogo porque
`module_pathname` é não-versionado.

## Gates

| Gate | Estado |
|---|---|
| Harness verde | OK — 4 cenários |
| Sem segredos | OK |
| Trailer de co-autoria ausente | OK |
| CHANGELOG atualizado | OK |
| Trabalho em `develop` | OK |
