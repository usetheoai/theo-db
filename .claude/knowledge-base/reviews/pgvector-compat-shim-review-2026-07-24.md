# Review — shim de compatibilidade pgvector (#181)

**Data:** 2026-07-24 · **Branch:** develop · **Issues:** #181 (corrigido), #182/#183/#184 (rastreados)
**ADR:** `docs/adr/0058-pgvector-compat-shim.md` · **Contexto:** desbloqueio técnico do M141 (dogfood)

## Verdict: **READY_TO_MERGE**

Sem BLOCKER. As duas findings HIGH foram **corrigidas e re-validadas** neste ciclo; o restante está
rastreado em issues com caminho técnico mapeado, e as limitações estão declaradas na ADR e no CHANGELOG.

## Agentes e achados

| Agente | Lente | Verdict |
|---|---|---|
| council-index-storage | storage / upgrade / dump-restore / packaging | 1 HIGH (corrigido), 6 MEDIUM, 3 LOW |
| council-security | privilégio / squatting / injeção / CASCADE / DROP | 0 BLOCKER; 1 MEDIUM (#183), 1 LOW-MEDIUM (#184) |

### HIGH-1 — `requires` + tooling sem `CASCADE` (CORRIGIDA)

O tooling real (drizzle/alembic/prisma/`pg_restore`) — e o próprio `theo-memory` (`package.json:30`) —
emite `CREATE EXTENSION IF NOT EXISTS vector` **sem** `CASCADE`. Com `requires = 'theodb_rs'` isso falha em
qualquer banco sem a dependência: `required extension "theodb_rs" is not installed`. Meu harness testava só
o caminho **com** `CASCADE` (o caso favorável) e a evidência de dogfood foi colhida num banco que já tinha
`theodb_rs` — **falso-verde meu**, reproduzido com o shim real antes de corrigir.

**Fix:** instalar a dependência em `template1` no initdb da imagem; todo banco criado depois a herda
satisfeita. **Re-validado:** banco criado depois do init → `CREATE EXTENSION IF NOT EXISTS vector` sem
CASCADE → `NOTICE: already exists, skipping`, tipo OK (`dist=1.7321`). Harness passou a cobrir esse caminho.

### HIGH-2 — o drop-in continua incompleto (RASTREADA, #182)

O shim **move** a falha do `CREATE EXTENSION` (linha 6) para o `CREATE INDEX` (linha 44) na migration real
do `theo-memory`: o AM `hnsw` e as opclasses `vector_*_ops` não existem. É progresso mensurável (as tabelas
`vector(N)` passam a ser criadas), não "app pgvector roda inteira". Correções de honestidade aplicadas:
CHANGELOG declara compatibilidade **parcial**; o harness declara o escopo #181 vs #182 na saída e no
cabeçalho; a ADR ganhou § Limitações conhecidas com os 5 achados.

### Segurança — nenhum BLOCKER (verificado no source do PG 18.4)

- **Privilégio:** control herda o default (`superuser = true`, `trusted = false`) — fail-closed e igual ao
  pgvector upstream. Registrado na ADR que `trusted = true` **não** deve ser adicionado (puxaria o
  `theodb_rs`, privilegiado com saída HTTP). → #183.
- **CASCADE:** `get_required_extension` recorre antes de `switch_to_superuser` — a required refaz a própria
  checagem com o usuário original. **Sem escalação.**
- **Injeção:** sem `EXECUTE`/`format`/SQL dinâmico; `pg_type`/`pg_namespace` resolvem em `pg_catalog`. Sem
  hijack de search_path.
- **`DROP EXTENSION vector`:** o shim não possui objetos; tipo e colunas sobrevivem; recriar é idempotente.
- **Squatting:** colisão de nome de arquivo com o pgvector upstream é real, mas o layout on-disk é
  byte-idêntico (ADR-0028 § D1) — confusão de identidade, não corrupção de memória. Declarado como cenário
  não suportado na ADR. → #184.
- **Dump/restore:** `pg_dump` transporta como COPY texto (round-trip lossless); `pg_upgrade` aborta antes de
  tocar dados (o shim não instala `.so`). **Sem caminho de corrupção silenciosa.**

## Dívidas fechadas neste ciclo

- Harness ganhou **chamador** (`check-compat` no `theodb_rs/isolation/Makefile`) — o próprio Makefile
  documentava esse pecado no `corrupt_index.sh`.
- Harness ganhou o caso **sem CASCADE**, o assert de `extversion = '0.5.1'` (trava bump sem script de
  upgrade — classe de defeito do M137) e `template0` para o teste de CASCADE seguir válido.

## Dívidas rastreadas (não fechadas)

#182 (AM/opclasse — o próximo bloqueio do M141), #183 (bootstrap de menor privilégio), #184 (hardening de
identidade), `scripts/migrate-smoke.sh` vermelho e estruturalmente impassável desde o M70 (precisa ser
reescrito para o procedimento `real[]` da ADR-0029 § D3 ou aposentado por ADR), extensão do
`schema-drift-gate.yml` para `vector.control`/`sql/vector--*.sql`.

## Gates de merge — verdes

Testes verdes (harness exit 0, não-vacuidade provada) · sem secrets · sem commit em `main` · sem
`Co-Authored-By` · CHANGELOG atualizado e **honesto**.

**Verdict:** READY_TO_MERGE
