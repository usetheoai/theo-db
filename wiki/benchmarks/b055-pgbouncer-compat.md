---
type: Measurement
title: b055 — PgBouncer nos três modos: o que sobrevive, o que vaza, e o que nunca foi problema
description: Sob transaction e statement pooling o ef_search de um cliente aplica-se às buscas de outro. A hipótese registrada previa PERDA do ajuste; a medição mostrou CONTAMINAÇÃO, que é pior. Os contadores de explain_scan não contaminam — hipótese refutada com o mesmo peso.
tags: [pgbouncer, pooler, compatibilidade, ecossistema, guc, honest-negative, b055]
item: B-055
generated: { by: claude-code/opus-5, at: 2026-08-20T00:00:00Z }
sources:
  - id: run
    resource: ghcr.io/usetheoai/theo-db:latest
    title: imagem lançada do produto, PostgreSQL 18.6, theodb_rs 1.5.0
    last_modified: 2026-08-20
---

O `README.md` afirmava, sem qualificação, que *"seus drivers, ferramentas e aplicações funcionam sem
mudança"*. Um pooler é uma ferramenta, e até esta medição ninguém tinha executado um contra o produto —
o tema não aparecia em `wiki/`, `README.md`, `PRD.md` nem `CHANGELOG.md`.

Conceito relacionado — o índice cujo `ef_search` é o parâmetro que vaza:
[HNSW](../technologies/hnsw.md).

# A matriz

PgBouncer 1.25.2 contra `ghcr.io/usetheoai/theo-db:latest`, `default_pool_size = 1` para que **os dois
clientes compartilhem o mesmo backend por construção** — sem isso o teste não discrimina nada.

| | `session` | `transaction` | `statement` |
|---|---|---|---|
| `SET theodb_hnsw.ef_search` de um cliente é visto por outro | **não** (64, o default) | **SIM (7)** | **SIM (7)** |
| `CREATE TEMP TABLE` de um cliente é visível a outro | não (relação não existe) | **SIM (1 linha)** | **SIM (1 linha)** |
| `PREPARE` de um cliente é executável por outro | — | **SIM** | **SIM** |
| contadores de `theodb.explain_scan` contaminam | — | **não** | — |

# A hipótese estava certa no eixo e errada na direção

O item previa **perda**: o `SET` fora de transação não sobreviveria à devolução da conexão, e a busca
seguinte rodaria com o default 64 em silêncio. A medição mostra o oposto e é pior: o valor **persiste no
backend** e passa a valer para o **próximo cliente que pegar aquela conexão**.

A diferença importa para quem opera. Perda degrada a busca de quem ajustou — quem mediu recall com
`ef=200` recebe o de 64 e vê o número piorar. Contaminação degrada a busca de quem **não ajustou nada**: um
cliente que nunca tocou o knob passa a rodar com o `ef` de outro, e o recall dele muda sem que nada no
código dele explique por quê.

## Isto não é defeito do TheoDB, e dizê-lo com precisão importa

Vazamento de estado de sessão sob transaction pooling é comportamento **documentado do PgBouncer** e vale
para qualquer PostgreSQL — temp table, prepared statement e `SET` sempre foram assim. O `server_reset_query`
(`DISCARD ALL`) só se aplica em session e statement pooling por padrão.

O que é **específico deste produto** é a consequência: são **42 GUCs**, e os de ajuste de busca decidem
recall. Num PostgreSQL sem extensão, o estado de sessão que vaza é `search_path` ou `work_mem`; aqui, é o
parâmetro que determina *quantos vizinhos a busca visita*. A propriedade é genérica; o custo, não.

# O honest-negative, com o mesmo peso

Os contadores por backend (`SCAN_PAGES_READ` / `SCAN_CANDIDATES`, que alimentam `theodb.explain_scan`)
**não contaminam**. Medido no mesmo backend (`pid=1337`): o cliente B rodou `ef=512` e reportou
`pages=1281 cand=952`; o cliente A, em seguida, com `ef=8`, reportou `pages=139 cand=62` — **idêntico** ao
valor lido direto no banco sem pooler. Eles são reiniciados por varredura, não acumulados por conexão.

A hipótese (b) do item está **refutada**, e o DoD exigia registrá-la com o mesmo peso de uma confirmação.

# Reprodução

```bash
docker network create b055net
docker run -d --name b055-db --network b055net -e POSTGRES_PASSWORD=postgres \
  ghcr.io/usetheoai/theo-db:latest
# corpus: 3000 linhas vector(8) + índice theodb_hnsw
# pgbouncer.ini: pool_mode = transaction, default_pool_size = 1
docker run -d --name b055-pg --network b055net \
  -v "$PWD/pgbouncer.ini:/etc/pgbouncer/pgbouncer.ini:ro" \
  -v "$PWD/userlist.txt:/etc/pgbouncer/userlist.txt:ro" edoburu/pgbouncer:latest

# cliente B define
psql -h b055-pg -p 6432 -U postgres -tAc "SET theodb_hnsw.ef_search = 7"
# cliente A, que nunca tocou o knob, lê
psql -h b055-pg -p 6432 -U postgres -tAc "SELECT current_setting('theodb_hnsw.ef_search')"
# -> 7 sob transaction/statement; 64 sob session
```

`default_pool_size = 1` é o que torna o teste determinístico. Com o default (`20`) e um cliente só, o
PgBouncer devolve sempre a mesma conexão de servidor e **tudo passa** — foi o primeiro resultado desta
medição, e ele não provava nada. Medir o `pg_backend_pid()` ao lado do valor é o que revelou que as
transações nunca tinham saído do mesmo backend.

# O que fazer

Sob **session pooling** nada muda: o TheoDB se comporta como qualquer PostgreSQL. Sob **transaction** ou
**statement**, o ajuste de busca precisa ser `SET LOCAL` dentro da transação que faz a consulta — que é a
mesma regra que já vale para `work_mem` e `search_path`, aplicada a um parâmetro que decide recall.

# Achado colateral

A montagem deste arnês encontrou [[B-089]] por acidente: um vetor zero na tabela derruba a busca por
cosseno no índice, no `ef_search` default. Nada a ver com pooler — o corpus sintético é que o expôs.
