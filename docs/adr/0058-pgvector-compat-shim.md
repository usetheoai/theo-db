# ADR 0058 — Shim de extensão `vector`: completar o drop-in pgvector no nível tooling

- **Status:** aceito
- **Data:** 2026-07-24
- **Issue:** [#181](https://github.com/usetheodev/theo-db/issues/181)
- **Depende de:** ADR-0028 (M69 — tipo `vector` own-code), ADR-0029 (M70 — `public.vector` drop-in + remoção do pgvector)
- **Descoberto por:** dogfood do M141 (`knowledge-base/dogfood/evidence/2026-07-24-anchor-bootstrap-blocker.md`)

## Contexto

O M70 (ADR-0029 § D2) decidiu que o tipo próprio ocupa `public.vector` **justamente para ser drop-in**:
*"`::vector` do usuário e o `FOR TYPE vector` das opclasses resolvem ao tipo próprio SEM mudança de código."*

Ao tentar apontar uma capability theo-data real (`theo-memory`) para um TheoDB self-hosted — o passo seguinte
do anchor de dogfood — a app **não subiu**. Medido no droplet (PG 18.4 + `theodb_rs` @ develop):

```
CREATE EXTENSION IF NOT EXISTS vector;
ERROR:  extension "vector" is not available
```

O tipo `public.vector` existe e `<->` funciona; o que falta é o **objeto de extensão nominal**. Toda app
pgvector executa `CREATE EXTENSION IF NOT EXISTS vector` no bootstrap — `theo-memory` no script `db:push`
(`package.json:30`) e em ≥7 testes de integração, `theo-rag` idem; ambas hoje em `ankane/pgvector:v0.5.1`.
O resultado: nenhuma capability consegue **inicializar**, então os "≥30 dias de tráfego real" do M141 nem
podiam começar a contar.

Diagnóstico honesto: a compatibilidade do M70 foi entregue e validada no nível **SQL/tipos**, mas o nível
**tooling/drivers** (um dos sete níveis de compatibilidade da skill `theodb-evolution`) nunca foi exercitado
— porque nenhuma aplicação real havia sido apontada para o banco. É o anti-pattern que a skill chama de
*"PostgreSQL-compatible como vibe"*, e só o dogfood o encontrou: 109+ artefatos de benchmark não o achariam,
pois nenhum deles inicializa uma aplicação.

## Decisão

Prover uma extensão `vector` **shim** — `vector.control` + `sql/vector--0.5.1.sql` — que **não implementa
nada**: o tipo, os operadores e as opclasses continuam sendo own-code do `theodb_rs`. O shim existe para que
`CREATE EXTENSION IF NOT EXISTS vector` suceda, completando no nível tooling o drop-in que a ADR-0029 § D2 já
havia decidido no nível SQL.

Propriedades:

- `requires = 'theodb_rs'` — `CREATE EXTENSION vector CASCADE` num banco limpo instala o `theodb_rs` sozinho
  (medido: `NOTICE: installing required extension "theodb_rs"`), e o PostgreSQL barra o `DROP EXTENSION
  theodb_rs` enquanto houver coluna `vector` (medido).
- `default_version = '0.5.1'` — declara o **contrato de features** que o tooling inspeciona (tipo `vector`,
  operadores de distância, índice ANN), que o TheoDB satisfaz. Não é alegação de ser o pgvector.
- **Honestidade (Regra 3):** o `comment` do control — visível em `\dx` e em `obj_description` — declara
  literalmente *"the vector type/operators/opclasses are provided by TheoDB own-code (theodb_rs), NOT by
  pgvector"*. O harness de regressão **asserta esse texto** (asserção 5), então a honestidade não pode
  regredir silenciosamente.
- **Fail-fast (Regra 8):** o script SQL não é vazio — valida que `public.vector` existe e, se não existir,
  levanta erro tipado (`undefined_object`) com HINT. Uma app nunca deve acreditar que tem pgvector e quebrar
  depois, de forma obscura, na primeira coluna `vector(N)`.

## Rationale e alternativas rejeitadas

**Alternativa A — pedir que cada app remova `CREATE EXTENSION vector` do seu bootstrap.** Rejeitada:
multiplica o atrito exatamente onde o dogfood precisa de zero atrito (cada capability, cada teste de
integração, cada ambiente), e contradiz o drop-in que a ADR-0029 § D2 já decidiu. O atrito de migração é o
principal motivo pelo qual dogfoods não acontecem.

**Alternativa B — reintroduzir o pgvector como dependência.** Rejeitada: contradiz o M70 (ADR-0029) e o
North Star (o TheoDB é o 1º AM permissivo com tipo `vector` 100% own-code). O shim mantém a implementação
own-code; só empresta o nome que o tooling exige.

**Alternativa C — publicar um `vector` com `default_version` fictício (ex.: `99.0`).** Rejeitada: tooling
que checa versão para decidir features (HNSW ≥ 0.5.0) receberia um número sem significado. `0.5.1` mapeia o
conjunto de features realmente oferecido.

**Parsimony ladder:** rung 3 (feature nativa da plataforma) — um control file + um script SQL é o mecanismo
que o próprio PostgreSQL oferece; nada foi reimplementado (Regra 9). Reusa o padrão SQL-only já estabelecido
no repo por `theodb.control`.

## Evidência (medida, droplet PG 18.4)

| Cenário | Antes | Depois |
|---|---|---|
| `CREATE EXTENSION IF NOT EXISTS vector` | `ERROR: extension "vector" is not available` | OK |
| `CREATE EXTENSION vector CASCADE` (DB limpo) | erro | instala `theodb_rs` + `vector` |
| tipo em `public` | ✅ já era | ✅ inalterado |
| `[1,1,1] <-> [2,2,2]` | ✅ já era | `1.7321` (=√3) |
| índice `theodb_hnsw` sobre a coluna | ✅ já era | ✅ criado |
| idempotência (2º deploy) | — | no-op |

Harness de regressão: `theodb_rs/isolation/pgvector_compat_check.sh` — exit 0 com o shim,
**exit 1 sem o shim** (não-vacuidade provada, reproduz o RED exato).

## Consequências

- Uma app pgvector existente passa a apontar para o TheoDB **sem alterar código**, removendo o bloqueio
  técnico do M141. O que resta para o M141 é estritamente operacional/temporal: ≥30 dias de tráfego real,
  dependência do time, ≥2 operadores.
- Nova superfície pública versionada (`vector 0.5.1`): mudar seu `default_version` passa a ser mudança de
  contrato com o tooling das apps.
- O shim NÃO torna o TheoDB compatível com o pgvector em tudo — apenas com o que o `theodb_rs` de fato
  provê. Recursos do pgvector fora dessa superfície continuam ausentes, e o `comment` do `\dx` é o aviso.
