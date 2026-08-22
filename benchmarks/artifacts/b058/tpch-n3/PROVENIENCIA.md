# Procedência — corrida com repetição (n=3)

`theodb-bench tpch --repetitions 3` no droplet `theo-bench-20260822T171853Z`
(`g-16vcpu-64gb`, nyc1), 2026-08-22, 17:26–17:35 UTC. Máquina destruída após a colheita.

- **TheoDB**: imagem `theodb:fix` construída no host a partir de `c934b5f`.
  `PostgreSQL 18.6`, lido do servidor.
- **AlloyDB Omni**: `google/alloydbomni:latest`, `PostgreSQL 17.9`, `--shm-size=4g`.
  `google_columnar_engine.enabled` ligado por `ALTER SYSTEM` + restart, com o valor `on`
  **lido de volta do servidor** antes das pernas `engineon`.

Cada JSON traz, por query: `seconds` (**mediana** das 3), `stdev_seconds`, `samples` (as
três, em ordem de execução), `matches_oracle` e `rows_returned`. **O oráculo foi conferido
em TODA repetição** — 90 verificações, todas concordantes.

**O que esta corrida NÃO tem:** o portão `assert_analytical_path` foi ligado na suíte TPC-H
*depois* que ela foi lançada, então nenhuma perna passou por prova de residência formal. A
evidência de que o caminho foi o declarado é indireta e está no próprio resultado — ver a
seção correspondente no conceito.

Ainda JSON cru, não bundle validado: o comando `tpch` não emite bundle ([[B-069]]).
