# M169 — byte-identidade colunar vs heap (100M)

Prova de **correção**, complementar à corrida de conclusão. Cada lado roda UMA vez, sem `LIMIT`
(empates escolheriam linhas arbitrárias entre válidas) e sem medição de tempo.

- idênticas: **4/4**
- divergentes: **0**
- não verificadas: **0**

| q | roteou | idêntico | linhas (colunar / heap) | colunar s | heap s | nota |
|---|---|---|---|---|---|---|
| q20 | sim | **sim** | 1 / 1 | 59.49 | 165.7 |  |
| q32 | sim | **sim** | 10 / 10 | 279.91 | 886.23 |  |
| q33 | sim | **sim** | 10 / 10 | 110.24 | 474.27 |  |
| q34 | sim | **sim** | 10 / 10 | 111.23 | 469.43 |  |

## Proveniência

| | |
|---|---|
| box | 16 vCPU / 31 GB, `data_directory=/srv/m169data` |
| corpus | `hits` colunar e `hits_heap` — 99.997.497 linhas cada |
| binário | `so_md5 = debde5f3911306c739e5459b2517d14d` |
| sessão | `theodb.enable_columnar_agg = on`, `work_mem = 256MB`, `statement_timeout = 4h` |

`statement_timeout` de 4h e não os 300 s do benchmark: o lado heap varre 66 GB, e um timeout aqui se leria como
"não deu para provar" quando seria só orçamento de tempo. Este documento prova **correção**, não velocidade.

**Não-vacuidade:** a coluna `roteou` vem de um `EXPLAIN` executado ANTES de cada comparação. Sem ela, uma sessão
sem `theodb.enable_columnar_agg` compararia um caminho que este milestone não toca e estamparia "idêntico" com
toda a razão e nenhuma relevância — foi o que aconteceu numa corrida descartada, denunciada pelo TEMPO (4m45s
contra 59,5 s), não pelo resultado.

**Ordem total, não remoção do `LIMIT`.** O empate é desfeito acrescentando as colunas de saída como critérios
posicionais. Remover o `LIMIT` — a forma anterior — pediu ~10⁸ linhas na q32 e fez o kernel matar o backend.

Reprodução:

```
PGDATABASE=postgres python3 benchmarks/m169_ab_verify.py --queries 20,32,33,34 \
    --timeout-ms 14400000 --out docs/benchmarks/m169-ab-verify.md
```
