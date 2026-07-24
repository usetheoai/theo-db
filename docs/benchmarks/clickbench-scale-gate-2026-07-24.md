# Gate de escala do ClickBench na DO — BLOQUEADO por defeito no colunar (#190)

**Data:** 2026-07-24 · **Box:** droplet DO `c-8` **dedicado e efêmero** (8 vCPU, 15 GB, 100 GB), criado para
esta medição e destruído ao fim · **Imagem:** `ghcr.io/usetheodev/theo-db:0.139.0` (PG 18 + `theodb_rs` 1.2.0)

## Objetivo

Validar, antes de gastar infraestrutura AWS, que o pipeline do ClickBench roda de ponta a ponta em escala
real na DO — com a amostragem corrigida (sistemática, cobrindo todo o arquivo) e o pushdown vetorizado
ligado (`--agg`, viável desde o fix do #135 no M131).

## Veredito: **BLOQUEADO** — e o gate pagou por si

O run não produziu números de latência. Produziu algo mais valioso: **um defeito que impede qualquer carga
real no `theodb_columnar`** (#190), encontrado por **~$0,05** de droplet efêmero em vez de ~$9 de AWS.

```
INSERT INTO hits SELECT * FROM hits_heap
psycopg2.errors.InternalError_: cannot fetch toast data without an active snapshot
```

## O que foi medido antes de bloquear

| Etapa | Resultado |
|---|---|
| Amostragem sistemática (1-em-99, varrendo o arquivo inteiro) | ✅ **1.000.000 linhas** materializadas |
| Carga no heap de controle (`hits_heap`) | ✅ 1.000.000 linhas |
| **Carga no `theodb_columnar`** | ❌ **0 linhas** — INSERT aborta |
| 43 queries ClickBench | não executadas (sem dados) |

## Caracterização do defeito (repro mínimo)

| Cenário | Resultado |
|---|---|
| Tabela nova, 1 INSERT de 1.000 / 2.000 / 4.000 / 8.000 | ✅ OK |
| Tabela nova, 1 INSERT de 20.000 | ❌ `cannot fetch toast data…` |
| Tabela existente, **2º** INSERT de 5.000 | ❌ `cannot fetch toast data…` |

O gatilho é o **segundo flush de stripe** na presença de valores TOAST. Condição necessária confirmada:
`max(octet_length(url)) = 3.951 bytes`, acima do `TOAST_TUPLE_THRESHOLD` (~2 KB).

Num sub-caso (`TRUNCATE` + INSERT repetidos) a relação ficou **ilegível** após o abort
(`could not read blocks 119..119 … read only 0 of 8192 bytes`) — marcado `[NEEDS-REPRO]` em #190; o erro
de INSERT em si é 100% determinístico.

## Por que isso nunca apareceu antes — e o que isso implica sobre M128/M131

Os runs anteriores amostravam com `head -n`: as **primeiras** N linhas de um dataset ordenado por
`EventTime`. Isso é uma fatia temporal estreita, sem valores largos o bastante para TOAST. Ao corrigir a
amostragem para percorrer o arquivo inteiro (1-em-K), os valores TOASTáveis entraram — e o defeito
apareceu de imediato.

**Consequência honesta para os números publicados:** os resultados do **M128** (geomean 5,567 s,
storage-path) e do **M131** (0,8962 s acelerado, **1,90×** full-suite) foram obtidos num regime de dados
que **não contém valores TOAST** e com cardinalidades de `GROUP BY` artificialmente baixas — exatamente o
cenário em que o pushdown vetorizado mais se destaca. Eles permanecem válidos como o que sempre
declararam ser (correção byte-idêntica 43/43 e ganho **naquele** regime), mas **não devem ser lidos como
previsão do desempenho em dados reais completos**. A magnitude do 1,90× em dados representativos é, hoje,
**desconhecida**.

## Consequência para o AWS (etapa d)

**Não executar.** Rodar o `c6a.4xlarge` com o #190 aberto gastaria ~$9 para reproduzir exatamente a mesma
falha: o dataset canônico completo tem muito mais valores TOAST que a amostra de 1M, e a carga é o
primeiro passo do protocolo. A sequência correta é: corrigir #190 → repetir este gate na DO → só então AWS.

## Custo real desta etapa

| Item | Valor |
|---|---|
| Droplet `c-8` efêmero (13 min) | **$0,05** |
| Droplet criado com chave errada, destruído em minutos | ~$0,01 |
| **Total** | **~$0,06** |

Nenhuma máquina permanece ligada: o droplet foi destruído imediatamente após a coleta, e a conta voltou às
duas instâncias permanentes (`theo-e2e-runner`, `theo-ci-runner`).

## Reprodução

```bash
doctl compute droplet create theo-clickbench-c --size c-8 --image ubuntu-24-04-x64 \
  --region nyc3 --ssh-keys <id> --tag-name ephemeral-bench --wait
# docker load da imagem theo-db, depois:
PYTHONPATH=benchmarks PGPORT=28900 python3 benchmarks/run_m128_clickbench.py \
  --n 1000000 --sample systematic --agg --out docs/benchmarks/gate-1m-agg-on.json
doctl compute droplet delete <id> --force   # SEMPRE
```
