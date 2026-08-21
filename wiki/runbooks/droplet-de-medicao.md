---
type: Runbook
title: O droplet de medição — o que já existiu, o que custa, e como subir um
description: Droplet efêmero g-16vcpu-64gb, agora nascido de um snapshot provisionado. ops/provision.sh é a fonte de verdade e o snapshot é cache. Registra as seis armadilhas de host limpo (~70 min perdidos numa sessão), a regra que as torna obsoletas — portão de capacidades antes do trabalho caro — e por que tarball limita todo veredito a EXPLORATORY.
tags: [runbook, medicao, droplet, custo, b-069, b-098, portao, snapshot]
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

# no host — provisiona E verifica. O `--verify` sozinho e idempotente e barato.
./theodb-bench/ops/provision.sh          # instala tudo, inclusive docker-buildx e o extra [postgres]
./theodb-bench/ops/provision.sh --verify # reprova em ~2 s se faltar qualquer capacidade

# o arnês NÃO sobe o servidor — ele mede um que já exista, no DSN `postgresql:///postgres`.
# Essa divisão é deliberada: o arnês mede, não faz deploy. O executor abaixo sobe o servidor,
# cria o diretório de Parquet (armadilha 5), prova a proveniência LENDO DO SERVIDOR, roda um
# smoke barato e só então libera o sweep caro.
SUITE=analytical/crossover/row-count TAGS="base fix" ./theodb-bench/ops/bench-run.sh

doctl compute droplet delete <id> --force        # SEMPRE
```

**Partindo do snapshot** (o caminho rápido; ver § A imagem de bench): o `provision.sh` já rodou, e
`--verify` é o que confirma isso antes de medir. Partindo de uma `ubuntu-24-04-x64` limpa, rode o
`provision.sh` inteiro — são ~40 s de pacotes mais o venv.

**Os parâmetros do servidor são declarados, não default.** Um `maintenance_work_mem` de 8 GB não é o
que um usuário recebe; ele está aqui porque o build de HNSW o usa, e um artefato que não declara isso
não é reproduzível. Ver [ADR-0064](../decisions/0064-maintenance-work-mem-nao-e-contrato.md) para por
que ele não é um contrato de memória neste produto.

# A imagem de bench — cache, não fonte de verdade

A prática passou a ser: **um snapshot com o host já provisionado**, do qual cada corrida nasce. O
owner aceitou o custo mensal em troca de velocidade (2026-08-21).

A inversão importa e é o que impede o snapshot de virar um *pet*: **`ops/provision.sh` é a fonte de
verdade; o snapshot é um artefato derivado dele.** Um snapshot é um blob opaco — em três meses
ninguém sabe o que tem dentro —, e este projeto se apoia no princípio oposto: até `shared_buffers` é
declarado, não default, porque artefato com estado não-declarado não é reproduzível. Se o snapshot
sumir ou envelhecer, roda-se o script e ele volta.

| | |
|---|---|
| o que ENTRA | SO, pacotes, Docker **com buildx**, venv do arnês com `[postgres,datasets]`, datasets verificados, cache de camadas Docker do toolchain (~13 min de build) |
| o que NÃO entra | imagens de medição específicas (`theodb:base`/`:fix` viram *pet*), e **credencial nenhuma** — snapshot é um disco restaurável por quem tiver acesso à conta |
| custo | ~US$ 0,06/GiB/mês; o host medido usa 18 GB → **~US$ 1,08/mês** |
| já correndo na conta | 43,62 + 19,81 GiB ≈ **US$ 3,81/mês** dos dois snapshots de 2026-08-19 |
| economia | ~14 min por corrida ≈ US$ 0,18 → equilíbrio em **~6 corridas/mês** |

Abaixo de ~6 corridas/mês o snapshot **dá prejuízo em dinheiro**. O argumento que o sustenta não é
tempo, é confiabilidade: as seis armadilhas acima custaram ~70 min numa única sessão — quase um mês
de snapshot, de uma vez.

# Tarball ou `git clone`: o que decide o veredito

O arnês valida `clean_source_tree`, e ele **reprova com código enviado por tarball**:

```
clean_source_tree   UNAVAILABLE   unavailable: git status failed
```

O [[b058-crossover-colunar]] carrega exatamente essa ressalva, e é por ela — junto com CPU set e
limite de memória não declarados — que seu veredito é `EXPLORATORY` e não `release`.

**Enquanto o código chegar por tarball ou por imagem publicada, nenhuma corrida deste projeto pode
ser `release`.** As três vias, com o teto de cada uma:

| via | serve para | teto de veredito |
|---|---|---|
| `git clone` num SHA | qualquer commit, inclusive não publicado | **`release`** (única via) |
| `docker pull` da imagem publicada | só código já publicado | `EXPLORATORY` |
| tarball via `scp` | commit não publicado, sem credencial no host | `EXPLORATORY` |

O `git clone` tem um pré-requisito **em aberto**: o host precisa de deploy key ou token de leitura, e
hoje não tem — foi por isso que a medição do [[B-097]] foi por tarball. Enquanto isso não existir, o
teto é `EXPLORATORY`, e dizer o contrário sobre qualquer número daqui seria falso.

# As seis armadilhas — e a regra que torna a lista obsoleta

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

4. **`COPY failed: no source files were specified` no passo 26 de 28.** O `Dockerfile` do `theo-db` usa
   `COPY <<'EOF'` (heredoc), que **só o BuildKit entende**. O pacote `docker.io` do Ubuntu 24.04 instala
   o daemon mas **não** o plugin `buildx`, então o `docker build` cai no builder legado e falha — depois
   de ter compilado a extensão inteira. `apt-get install docker-buildx` resolve.

   Esta custou **40 minutos de droplet ocioso** (~US$ 0,50), e o desperdício não foi o build falhar: foi
   ele falhar **no fim**. A lição não é "instale buildx", é **onde o portão fica**. Um teste de
   capacidade do host pertence ao começo do script, antes do trabalho caro:

   ```bash
   docker buildx version >/dev/null 2>&1 || { echo "FALHA: buildx ausente"; exit 1; }
   ```

   Falhar ali custa 2 segundos. Falhar no passo 26 custa a compilação. É a mesma família das três acima
   — coisa que só aparece quando nada é herdado — e é a razão de a lista ter crescido em vez de a
   armadilha virar nota de rodapé.

5. **O diretório de Parquet não existe, e o arnês culpa o servidor.** O adapter escreve em
   `/var/lib/postgresql/theodb-bench-parquet` **pelo processo servidor**, então ele precisa existir
   DENTRO do contêiner e pertencer ao usuário do banco. O procedimento acima não o cria. Sem ele:

   ```
   ERROR: theodb.write_parquet: criar '…/bench_analytical_parquet.parquet.84.tmp': No such file or directory
   ```

   e o veredito do arnês vira `INVALID` por **`sut_alive` FAIL — "o sistema sob teste caiu ou ficou
   inalcançável"**. O servidor estava de pé e `healthy` o tempo todo; quem falhou foi uma consulta.
   **Um portão que aponta o culpado errado custa mais que a falha que ele reporta** — é o achado mais
   caro desta lista, porque manda quem diagnostica para o lado errado. Registrado como [[B-098]].

   ```bash
   docker exec -u root theodb mkdir -p /var/lib/postgresql/theodb-bench-parquet
   docker exec -u root theodb chown postgres:postgres /var/lib/postgresql/theodb-bench-parquet
   ```

6. **Um comando de REGISTRO derrubando a corrida.** Um `psql` de proveniência usando `theodb` — nome
   do umbrella que deixou de existir no B-030; hoje é `theodb_rs` — retornou não-zero e, sob
   `set -e`, matou o script. **29 minutos de droplet ocioso** para uma linha que não media nada.

   A regra que sai disto organiza o executor inteiro: **o que MEDE aborta em erro; o que apenas
   REGISTRA nunca aborta.** Toda linha de proveniência termina em `|| true`.

# A regra, que vale mais que a lista

As seis têm a mesma forma: **capacidade que existe na máquina de desenvolvimento e não num host
limpo, descoberta DEPOIS do trabalho caro.** Uma lista de armadilhas não impede a sétima — ela só
registra as seis que já doeram.

O que impede é um **portão de capacidades executável, rodado antes de qualquer trabalho caro**:

```bash
theodb-bench/ops/provision.sh --verify     # reprova em 2 s, ou libera
```

E o corolário sobre ONDE o portão fica: falhar no início custa segundos; falhar no passo 26 de 28
custa a compilação inteira. Medido nesta sessão: **~70 min de host pago desperdiçados (~US$ 0,88),
com zero medições produzidas.**

Depois do portão vem o **smoke**: uma suíte barata (`analytical/synthetic/paths`, um N, três
caminhos, ~35 s) que exercita o pipeline inteiro antes de liberar um sweep que carrega 2 milhões de
linhas seis vezes. Ambos vivem em `theodb-bench/ops/bench-run.sh`.

# O que NÃO fazer

- **Deixar o droplet de pé.** US$ 0,75/h é barato por corrida e caro por semana. `delete --force` faz
  parte do procedimento, não é o passo opcional do fim.
- **Tocar no `theo-e2e-runner` ou no `theokit-website`.** Não são de medição.
- **Publicar número que não saiu do arnês.** É o [[B-069]], e a razão de este runbook existir.
- **Medir com parâmetro de servidor não declarado.** Um artefato sem os `-c` acima não é reproduzível.
- **Tratar a lista de armadilhas como o conserto.** Ela é o registro; o conserto é o portão de
  capacidades — ver § A regra.
