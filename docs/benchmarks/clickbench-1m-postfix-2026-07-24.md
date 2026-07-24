# ClickBench 1M pós-#190 — o gate DESTRAVOU (medido)

**Data:** 2026-07-24 · **Box:** droplet DO `c-8` **dedicado e efêmero** (8 vCPU, 15 GB), destruído ao fim ·
**Imagem/binário:** `theodb_rs` @ develop com o fix do #190 (`5228b0d`), PG 18.4 pgrx-install ·
**Amostragem:** **sistemática** (1-em-99, varrendo o arquivo inteiro — a correção do viés `head -n`) ·
**Artefato bruto:** [`gate-1m-postfix.json`](./gate-1m-postfix.json)

## Contexto

O gate de `clickbench-scale-gate-2026-07-24.md` estava **bloqueado** — a carga de 1M linhas com TOAST
abortava com `cannot fetch toast data without an active snapshot` (#190). Com o fix (materialização de
TOAST na ingestão), a carga passa e as 43 queries rodam. Este é o primeiro número do pilar colunar em
dados **representativos** (amostra sistemática, não a fatia enviesada dos runs M128/M131).

## Resultado

| Métrica | Valor |
|---|---|
| Carga de 1.000.000 linhas (real `hits`, TOAST presente) | ✅ **completou** (antes: abortava) |
| Queries executadas | **42 / 43** completadas + **1 timeout** (q28, `REGEXP_REPLACE`, teto de 60 s) |
| Erros de correção | **0** |
| **A/B byte-idêntico vs heap** (entre as 42 completadas) | **42 / 42** — 0 divergências |
| CustomScan (pushdown vetorizado) engajado | **6 / 42** |
| hot geomean — as 6 com pushdown | **0,476 s** |
| hot geomean — as 36 sem pushdown | **47,25 s** |
| **hot geomean — todas as 42** | **24,5 s** |

## Leitura honesta

O fix resolve o bloqueio: a carga funciona e a correção é preservada (A/B 42/42). Mas o número revela o que
os benchmarks anteriores escondiam:

- **Só 6 das 43 queries engajam o pushdown vetorizado** — e essas são rápidas (geomean 0,476 s). As outras
  **36 rodam pelo executor row-based do PostgreSQL sobre o storage colunar**, a ~47 s cada. O geomean geral
  (24,5 s) é dominado por elas.
- O 1,90× do M131 e o 5,567 s do M128 foram medidos numa amostra `head -n` (fatia temporal, sem TOAST,
  cardinalidades baixas) — o regime que favorece o pushdown. **Em dados representativos, o colunar do TheoDB
  não é competitivo em escala para as queries que não engajam o CustomScan.** Isso não é falha do fix; é o
  estado real do pilar colunar, agora medível.

## Implicação para a roadmap

O pushdown vetorizado cobre 6/43 queries. Para o colunar ser competitivo em ClickBench, o CustomScan
precisa engajar em muito mais queries (hoje: contagens/agregações simples). As 36 queries a ~47 s são o
alvo real de otimização — não a amostra pequena onde tudo já era rápido. **Não submeter ao leaderboard**
com este número; o box não é o canônico `c6a.4xlarge` e o resultado não seria competitivo.

## Reprodução

```bash
doctl compute droplet create theo-clickbench --size c-8 --image ubuntu-24-04-x64 \
  --region nyc3 --ssh-keys <id> --tag-name ephemeral-bench --wait
# build+install theodb_rs @ develop, subir PG, depois:
PYTHONPATH=benchmarks PGPORT=28900 python3 benchmarks/run_m128_clickbench.py \
  --n 1000000 --sample systematic --agg --out docs/benchmarks/gate-1m-postfix.json
doctl compute droplet delete <id> --force   # SEMPRE
```
