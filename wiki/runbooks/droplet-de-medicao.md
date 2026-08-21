---
type: Runbook
title: O droplet de medição — o que já existiu, o que custa, e como subir um
description: A prática é droplet efêmero g-16vcpu-64gb destruído após cada corrida. Dois snapshots de 2026-08-19 sobrevivem. Registra também os dois defeitos que só um host limpo revela e que custaram três corridas em 2026-08-21.
tags: [runbook, medicao, droplet, custo, b-069]
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

Conceito irmão: [b018 — o planner larga o HNSW na junção](../benchmarks/b018-planner-hnsw-juncao.md),
a primeira medição feita por este procedimento.

# O que já existiu

**Sim, houve servidor dedicado — várias vezes, e sempre efêmero.** A prática, declarada pelo owner em
2026-08-13, é subir uma máquina, medir, e destruir. O `BACKLOG.md` registra pelo menos quatro IPs já
usados e destruídos: `164.90.141.31` (b035), `167.172.229.34` (lexical), `138.197.22.192` (DISCOVER
de 2026-08-17) e o `45.55.142.149` desta rodada.

Dois **snapshots pré-destruição** sobrevivem na conta, de 2026-08-19:

| snapshot | região | disco mínimo | tamanho |
|---|---|---|---|
| `theo-b059-bench-pre-destroy-20260819` | nyc3 | **320 GB** | 43,62 GiB |
| `theo-b076-build64-pre-destroy-20260819` | nyc1 | 200 GB | 19,81 GiB |

**O do b059 não cabe no `g-16vcpu-64gb`**, que tem 200 GB de disco — restaurá-lo exige um plano maior.
O do b076 cabe.

# A máquina, e o custo

| | |
|---|---|
| tamanho | `g-16vcpu-64gb` — 16 vCPU, 64 GB, 200 GB, **US$ 0,75/h** |
| região | `nyc3` (as corridas anteriores; comparabilidade importa mais que latência) |
| custo por corrida | **US$ 1 a US$ 2**, 15 a 30 min — medido, não estimado |
| chave SSH | `paulo-workstation-2026-08` (id `58598100`) |

A conta tem outros dois droplets que **não são de medição e não se toca**: `theo-e2e-runner` e
`theokit-website`.

# O procedimento

```bash
doctl compute droplet create theo-<item>-<data> \
  --region nyc3 --size g-16vcpu-64gb --image ubuntu-24-04-x64 \
  --ssh-keys 58598100 --tag-names theo-test,ephemeral --wait

# no host
apt-get install -y docker.io python3-venv postgresql-client git

# o arnês NÃO sobe o servidor — ele mede um que já exista, no DSN `postgresql:///postgres`.
# Essa divisão é deliberada: o arnês mede, não faz deploy.
mkdir -p /var/run/postgresql && chmod 777 /var/run/postgresql
docker run -d --name theodb -e POSTGRES_HOST_AUTH_METHOD=trust \
  -v /var/run/postgresql:/var/run/postgresql --shm-size=8g \
  ghcr.io/usetheoai/theo-db:develop \
  -c shared_buffers=16GB -c maintenance_work_mem=8GB \
  -c max_parallel_maintenance_workers=8 -c work_mem=256MB -c max_wal_size=8GB

export PGUSER=postgres          # ver § As três armadilhas, item 3
theodb-bench dataset fetch sift-128-euclidean
theodb-bench run <benchmark> --system theodb --profile research --dataset sift-128-euclidean

doctl compute droplet delete <id> --force        # SEMPRE
```

**Os parâmetros do servidor são declarados, não default.** Um `maintenance_work_mem` de 8 GB não é o
que um usuário recebe; ele está aqui porque o build de HNSW o usa, e um artefato que não declara isso
não é reproduzível. Ver [ADR-0064](../decisions/0064-maintenance-work-mem-nao-e-contrato.md) para por
que ele não é um contrato de memória neste produto.

# As três armadilhas, e as três custaram corrida

Medidas em 2026-08-21, provisionando do zero pela primeira vez em muito tempo. **Todas só aparecem em
host limpo** — é por isso que passaram despercebidas: toda corrida anterior herdou estado.

1. **O fetch do dataset dava 403.** A origem do `sift-128-euclidean` recusa o `User-Agent` default do
   `urllib` (`Python-urllib/3.12`) e aceita o do `curl`. O CDN filtra por agente. Corrigido no arnês —
   o agente agora identifica o cliente, sem fingir ser navegador.

2. **`h5py` não vinha instalado.** O extra é `pip install '.[datasets]'`, e sem ele o erro é claro.
   Não é defeito, é uma linha a mais no procedimento.

3. **`FATAL: role "root" does not exist`.** O DSN `postgresql:///postgres` não nomeia usuário, então o
   libpq usa o usuário do SO — `root` — e a imagem só tem o papel `postgres`. **`export PGUSER=postgres`**
   resolve, e é a solução certa: `PGUSER` é a variável nativa do libpq, não um remendo do arnês.

   Esta custou **duas** corridas, e não precisava: a causa estava no log do servidor desde o primeiro
   segundo, mas o arnês reportava só `could not connect to theodb [phase=bootstrap system=theodb]`. A
   causa ia para o JSON e sumia do texto. Corrigido — quem lê um log lê texto.

# O que NÃO fazer

- **Deixar o droplet de pé.** US$ 0,75/h é barato por corrida e caro por semana. `delete --force` faz
  parte do procedimento, não é o passo opcional do fim.
- **Tocar no `theo-e2e-runner` ou no `theokit-website`.** Não são de medição.
- **Publicar número que não saiu do arnês.** É o [[B-069]], e a razão de este runbook existir.
- **Medir com parâmetro de servidor não declarado.** Um artefato sem os `-c` acima não é reproduzível.
