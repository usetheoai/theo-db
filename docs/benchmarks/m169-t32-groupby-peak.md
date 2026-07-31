# M169 T3.2 — o pico do `GROUP BY` que o streaming **não** reduz

**Veredito: o pico é LINEAR na cardinalidade, e 2M grupos consomem 95,4% de uma pool de 192 MiB.**

## Proveniência

| | |
|---|---|
| box | 16 vCPU / 31 GB, `/srv/m169data` |
| `so_md5` | `5ba1e09efa3dcc41` |
| binário | `theodb.enable_columnar_agg_stream = on` (default) |
| dado | `t_peak`, 2.000.000 linhas, 200 chunk-groups |
| `work_mem` | 64 MB → pool `work_mem*2 + 64 MiB` = **192 MiB** |
| backends antes | 0 · loadavg 0,53 |
| trace | `THEODB_ADMIT_TRACE=1`, injetado por restart e **removido depois** |

## Medido

| grupos distintos | `peak_reserved` (bytes) | MiB | % do teto |
|---|---|---|---|
| 10³ | 220.864 | 0,21 | 0,1 % |
| 10⁵ | 8.458.304 | 8,1 | 4,2 % |
| 10⁶ | 96.108.608 | 91,7 | 47,7 % |
| 2×10⁶ (uma chave) | 192.086.080 | 183,2 | **95,4 %** |
| 2×10⁶ (duas chaves) | 167.028.800 | 159,3 | 83,0 % |

`reserved_at_end = 0` em todas — a pool é devolvida ao fim de cada consulta; não há vazamento.

De 10⁵ → 10⁶ (10× a cardinalidade) o pico cresce **11,4×**; de 10⁶ → 2×10⁶ (2×), cresce **2,0×**. A constante
implícita é ~**92 B por grupo distinto**.

## O que isto decide sobre a q32 do ClickBench

O M169 removeu o termo O(N) do **decode**. Este número mostra o que **permanece**: a tabela de hash é
O(grupos distintos).

A q32 (`GROUP BY WatchID, ClientIP`) tem chave quase-única sobre 100M linhas. A ~92 B/grupo, o estado seria da
ordem de **9 GB** — contra 192 MiB de pool no default, ou ~576 MiB mesmo com `work_mem = 256MB`. **O estouro não
é hipótese; é aritmética a partir da reta medida.**

Por isso a q32 aparece no baseline com `agg_routed = true` **e** `timeout`: ela **roteia** pelo caminho colunar e
morre no **estado**, não nos offsets — ao contrário de q20/q33/q34, que morriam no teto de offsets `i32` e são o
que o T2.1 endereça. Duas causas, sintomas parecidos.

## Duas honestidades

**Capturamos 6 linhas de trace, não as 5 previstas.** O `EXPLAIN` de verificação também roteia e emite trace.
Não invalida os números — cada linha traz o próprio pico —, mas o contador esperado estava errado, e reportar
"5 de 5" seria maquiar.

**Duas chaves deram pico MENOR que uma (159 vs 183 MiB) com a mesma cardinalidade.** Contraintuitivo; a causa
provável é o layout do hash para duas colunas `bigint`. **Não medi o suficiente para afirmar**, e a diferença não
muda a conclusão. Fica aberto em vez de explicado por conveniência.

## Reprodução

```bash
bash benchmarks/m169_groupby_peak.sh          # faz o restart com trace, mede, e restaura sem ele
```

Saída bruta: `docs/benchmarks/m169-artifacts/t32-groupby-peak.log`.
