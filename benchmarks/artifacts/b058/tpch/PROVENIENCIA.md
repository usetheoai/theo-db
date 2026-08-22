# Procedência destes artefatos

Produzidos por `theodb-bench tpch` (o arnês) no droplet `theo-bench-20260822T162158Z`
(`g-16vcpu-64gb`, nyc1), em 2026-08-22, entre 16:28 e 16:30 UTC. A máquina foi destruída
após a colheita.

- **TheoDB**: imagem `theodb:fix`, construída no host a partir de `9d0d4ea`.
  Servidor: `PostgreSQL 18.6 (Debian 18.6-1.pgdg12+2)`, lido do servidor.
- **AlloyDB Omni**: `google/alloydbomni:latest`.
  Servidor: `PostgreSQL 17.9`, lido do servidor. `--shm-size=4g`.
  `google_columnar_engine.enabled` foi ligado por `ALTER SYSTEM` + restart e o valor `on`
  foi **lido de volta do servidor** antes das corridas `engineon`.

**Estes arquivos são JSON cru, não bundles validados por schema.** O comando `tpch` não
emite bundle — ele imprime o resultado. Isso é uma lacuna registrada, não um descuido:
ver [[B-069]]. O que eles carregam: por query, o tempo em segundos, `matches_oracle` e o
número de linhas devolvidas. **Toda resposta bateu com o oráculo em todas as dez corridas.**

**Uma execução por ponto.** Não há repetição, portanto não há variância nem teste de
significância. Qualquer leitura destes números tem de carregar essa ressalva.
