# Review — M1 Core + empacotamento

**Slug:** m1-core-packaging
**Data:** 2026-06-28
**Verdict:** READY_TO_MERGE
**Plano:** `.claude/knowledge-base/plans/m1-core-packaging-plan.md` (plan-confidence SHIPPABLE 100)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m1-core-packaging-blueprint.md`
**Commits sob review:** `0a33962` (feat M1) + `24a9b02` (review hardening)

## Escopo

M1 formaliza a **distribuição PostgreSQL-compatível** do TheoDB: engine PGDG `postgresql-17` (17.10)
**não-forkado** (ADR 0001), empacotado com as extensões do MVP e evidência de qualidade. Três DoDs.

## DoDs — evidência verificada ao vivo

| DoD | Critério | Evidência | Status |
|---|---|---|---|
| **DoD-1** | Suíte de regressão PG17 upstream passa 100% na distribuição | `# All 225 tests passed.` via `packaging/Dockerfile.regress` (`FROM theo-db:dev`, `pg_regress`+`regress.so` da tag `REL_17_10`) + `packaging/run-regress.sh` contra cluster TheoDB efêmero. Engine-under-test assertado = PGDG 17.10. | ✅ verificado ao vivo |
| **DoD-2** | Extensões MVP pré-instaladas e habilitáveis | `vector` 0.8.3, `vectorscale` 0.9.0, `plpython3u`, `plpgsql` — todas reportam versão via `pg_extension` após `CREATE EXTENSION` em container fresco. Doc `docs/packaging/packaging-and-tuning.md`. | ✅ verificado ao vivo |
| **DoD-3** | Due-diligence de licença — zero AGPL na distribuição (D1, PRD §11) | `packaging/license-sweep.sh` (reprodutível, exit≠0 em AGPL real): (a) apt → só falso-positivo `ca-certificates` (GPL-2+/MPL-2.0); (b) **293 crates Rust** do pgvectorscale via `cargo metadata` → 0 AGPL/Affero, 100% permissivo. Evidência commitada em `docs/packaging/license-audit.md`. | ✅ verificado ao vivo |

## Achados e resolução

### HIGH-1 — Evidência de licença das crates Rust não-commitada/não-reprodutível (DoD-3) — RESOLVIDO

O sweep original rodou ad-hoc; o resultado não estava commitado nem re-executável. **Fix:** criado
`packaging/license-sweep.sh` (determinístico, pinned commit `57c88b7...`, exit≠0 em qualquer AGPL/Affero
real) + `docs/packaging/license-audit.md` com a distribuição completa das 293 crates. Re-rodado:
`LICENSE SWEEP PASSED — zero AGPL`. O gate agora é auditável e roda em CI.

### HIGH-2 — Ferramenta `loop-check-licence` substituída sem ADR/justificativa (DoD-3) — RESOLVIDO

A ROADMAP DoD-3 nomeia `loop-check-licence`. Implementamos o gate com um sweep determinístico/reprodutível
em vez do plugin multi-agente. **Fix:** desvio documentado em `docs/packaging/license-audit.md § Tool note`
com a justificativa (a pergunta é binária "há AGPL no que enviamos?"; um script sobre os pacotes apt da
imagem + a árvore de crates pinada é re-executável em CI e produz artefato estável — gate mais forte que um
audit LLM não-pinado). `loop-check-licence` permanece disponível para auditorias de proveniência periódicas.

### MEDIUM-1 — Inversão de ordem de dependência (M1 implementado após M2/M3/M4) — A REGISTRAR NO RELEASE

M1 (Core) é dependência conceitual de M2/M3/M4, mas foi formalizado por último. Isso é uma **inversão de
ordem de roadmap**, não um defeito de implementação: M2/M3/M4 já dependiam de fato da mesma imagem
`theo-db:dev` (PGDG 17 + extensões) que M1 agora documenta/prova. O release de M1 legitima retroativamente
a base sobre a qual M2/M3/M4 já foram entregues. **Ação:** registrar a inversão no `roadmap-runs/M1-*.md`
no momento do `/release` (rastreabilidade honesta — Regra 3).

### LOW (corrigidos em `24a9b02`)

- `run-regress.sh`: adicionado assert de versão do engine (`grep -q "17\.10"`) — prova que o engine-under-test
  é a distribuição, não um postgres divergente.
- CI: `timeout-minutes: 25` nos jobs pesados `pg-regression` e `ha-smoke` (guarda contra hang).
- Doc: qualificador "CORE regression suite (`src/test/regress`)" no headline + `CREATE EXTENSION IF NOT EXISTS`
  no snippet de reprodução + escopo `check-world` marcado como hardening futuro (honestidade de escopo).

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Testes passando na branch | ✅ (suíte de regressão 225/225) |
| Sem secrets commitados | ✅ (`sk-proj` staged = 0; `.env` gitignored, não-tracked) |
| Sem commit direto na `main` | ✅ (trabalho em `develop`) |
| Sem trailer Co-Authored-By | ✅ |
| CHANGELOG atualizado | ✅ (entrada M1 em `[Unreleased]`) |

## Veredito

**READY_TO_MERGE.** 3/3 DoDs com evidência verificada ao vivo. HIGH-1 e HIGH-2 corrigidos e re-verificados.
MEDIUM-1 é inversão de ordem de roadmap (não-bloqueante) a registrar no release. LOW corrigidos.
Próximo passo: `/release` → v0.5.0; flip auditável de `### M1 — [ ]` → `[x]` registrando a inversão de ordem.
