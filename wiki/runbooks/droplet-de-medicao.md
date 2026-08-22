---
type: Runbook
title: O droplet de medição — o que já existiu, o que custa, e como subir um
description: Droplet efêmero g-16vcpu-64gb, agora nascido de um snapshot provisionado. ops/provision.sh é a fonte de verdade e o snapshot é cache. Registra as seis armadilhas de host limpo (~70 min perdidos numa sessão), a regra que as torna obsoletas — portão de capacidades antes do trabalho caro — e por que tarball limita todo veredito a EXPLORATORY.
tags: [runbook, medicao, droplet, custo, b-069, b-098, portao, snapshot]
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

Conceito irmão: [b018 — o planner larga o HNSW na junção](../benchmarks/b018-planner-hnsw-juncao.md),
a primeira medição feita por este procedimento.

**Os números deste runbook — e os nove defeitos que só a execução revelou — vivem em
[b098](../benchmarks/b098-host-de-bench-medido.md).** Aqui está *como se faz*; lá está *o que foi
medido*, inclusive por que dois perfis do arnês estavam mortos por construção.

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

**Um comando, da máquina de desenvolvimento.** É o caminho provado ponta a ponta, e o único que
garante a destruição do droplet:

```bash
cd theodb-bench/ops
SUITE=analytical/crossover/row-count TAGS="base:HEAD~1 fix:HEAD" ./bench-droplet.sh
```

Ele cria o droplet a partir do snapshot, envia o arnês, roda o portão de capacidades, **constrói uma
imagem por ref git**, roda o smoke barato, mede, colhe em `./resultados/` e destrói. Medido: **~2 min
até a primeira medição** partindo do snapshot.

`TAGS` na forma `nome:ref` é o que permite comparar dois commits com honestidade — mesma máquina,
mesmo dia, mesmos parâmetros, diferindo só no código. `MANTER=1` mantém o host de pé para depuração,
avisando o custo por hora.

**A destruição é `trap EXIT`, não uma linha no fim** — e é a diferença que decidiu o desperdício de
2026-08-21: o dinheiro não foi embora num droplet caro, foi num droplet **ocioso**, depois de o
script morrer. Um droplet cujos resultados **não** foram colhidos não é destruído: ficar de pé
cobrando é ruim, destruir dado que custou uma hora de host é pior e é irreversível.

## O caminho manual, quando se quer olhar de perto

```bash
doctl compute droplet create theo-<item>-<data> \
  --region nyc1 --size g-16vcpu-64gb --image <snapshot-ou-ubuntu-24-04-x64> \
  --ssh-keys 58598100 --tag-names theo-test,ephemeral --wait

# NO HOST, depois de enviar ops/ e o arnês em /root/bench:
/root/provision.sh --verify   # ~2 s; reprova se faltar qualquer capacidade
/root/provision.sh            # só se o --verify reprovar

# o arnês NÃO sobe o servidor — ele mede um que já exista, no DSN `postgresql:///postgres`.
# Essa divisão é deliberada: o arnês mede, não faz deploy. O executor abaixo sobe o servidor,
# cria o diretório de Parquet (armadilha 5), prova a proveniência LENDO DO SERVIDOR, roda um
# smoke barato e só então libera o sweep caro.
SUITE=analytical/crossover/row-count TAGS="base fix" /root/bench-run.sh

doctl compute droplet delete <id> --force        # SEMPRE
```

Neste caminho a destruição é sua responsabilidade, e é exatamente por isso que ele não é o padrão.

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

## Medido em 2026-08-21: quanto o snapshot economiza

Mesmo ref (`3253f86`), mesmo tamanho de droplet, mesma região. Os dois caminhos medidos de ponta a
ponta:

| etapa | host limpo | do snapshot |
|---|---|---|
| provisionamento | 57 s | **9–18 s** |
| build da imagem | **15 min 40 s** | **11–16 s** — todas as camadas `CACHED` |
| até a primeira medição | ~17 min | **~2 min** |

**O cache de camadas do Docker sobrevive ao snapshot**, que era a incógnita que sustentava ou
derrubava o custo mensal. O build fica ~58× mais rápido.

O ganho que importa não é a soma dos minutos: é que medir deixa de ser um compromisso de vinte
minutos e vira algo de dois. Com equilíbrio em ~6 corridas/mês, o custo se paga; abaixo disso o
argumento é confiabilidade, não dinheiro.

### O que NÃO sobrevive: `/var/run`

Das nove capacidades verificadas, **oito sobreviveram ao snapshot e uma não**: `/var/run/postgresql`.
`/var/run` é **tmpfs** — recriado vazio a cada boot, por construção. Não é defeito do snapshot; é o
Linux funcionando como projetado, e supor persistência ali era erro de quem escreveu o script.

O conserto é o mecanismo nativo, não reprovisionar: uma regra `tmpfiles.d`, que o `provision.sh` passa
a instalar. Enquanto o snapshot for anterior a ela, o portão detecta a ausência e o script se cura em
~18 s — que é a arquitetura fazendo o que promete: **o script é a verdade, o cache se atualiza.**

### Um snapshot carrega o que estava no disco

O snapshot foi tirado de um host que já havia medido, então trouxe `~1 MB` de resultados antigos em
`/root/res-*`. A coleta que varria `res-*` juntava corrida velha com nova — **pior que não colher,
porque parece completo.** O `bench-run.sh` passou a registrar qual corrida acabou de rodar, e a coleta
leva só essa.

# O teto de veredito, medido — e não é o que eu supunha

Duas coisas separadas limitavam o veredito. **Uma eu consertei; a outra é da máquina e não tem
conserto em nuvem.**

## `clean_source_tree`: era o arnês, não o `theo-db` — e está resolvido

O portão roda `git status --porcelain` na árvore **do próprio arnês** (o campo é `benchmark_dirty`, e
a descrição diz *"Benchmark source tree was committed"*). Enviar o arnês por tarball não leva `.git`,
então `git status` falha e o portão fica `UNAVAILABLE`.

**Isso nunca precisou de deploy key.** Um `git bundle` é um repositório completo num arquivo: o host
clona dele e fica com árvore limpa num SHA conhecido, sem credencial e sem rede. Medido:

| envio | `git status` | `HEAD` | portão |
|---|---|---|---|
| tarball | rc=128, falha | — | `UNAVAILABLE` |
| clone do bundle | rc=0 | `61552a6f` | **`PASS`** |

O `bench-droplet.sh` passou a enviar bundle. A afirmação anterior deste runbook — de que só `git
clone` do GitHub levantaria o teto, e que faltava uma credencial — **estava errada**, e fica
registrada aqui em vez de apagada, porque baseou uma recomendação ao owner.

## `cpu_governor`: o teto é a CLASSE DE MÁQUINA, e vale para tudo já publicado

Medido em 2026-08-21 num `g-16vcpu-64gb` com swap desligado e `cpupower` tentado:

```
N/A * cpu_governor    unavailable: cpufreq governor not exposed
Host may NOT run a 'release' benchmark. Blocking: cpu_governor
```

`cpufreq` não é exposto ao hóspede numa VM, e `is_blocking_for` trata **qualquer coisa que não seja
PASS** como bloqueio quando o perfil declara o check obrigatório — o que `release` faz.

**Consequência para o projeto inteiro, e ela é maior que este item: nenhum número medido em droplet
DigitalOcean pode ser `publishable` pelas regras do próprio arnês.** Isso inclui os já publicados —
todos saíram de droplets. Nada disso os invalida como evidência; significa que a palavra correta para
eles é `EXPLORATORY` ou `research`, e que dizer "release" sobre qualquer um seria falso.

Tudo o mais do host passa: SMT desligado, swap desligado, NUMA único, 16 núcleos físicos. É um bom
host de medição — exceto na única coisa que uma VM não entrega.

**`nightly` é o teto em nuvem, e agora está ALCANÇADO.** Ele não exige governor, mas exige CPU set e
limite de memória declarados **e aplicados** — e chegar lá exigiu consertar duas coisas no arnês
(§ abaixo). Medido em 2026-08-21:

```bash
SUITE=analytical/crossover/row-count TAGS="base:HEAD~1 fix:HEAD" \
PROFILE=nightly CPU_SET="0-11" MEM_MAX="48G" ./bench-droplet.sh
```

```
VEREDITO: VALID
  repetitions_completed  PASS   required=True
  process_containment    PASS   required=True
  cpu_limit              PASS   required=True
  memory_limit           PASS   required=True
  clean_source_tree      PASS
```

Para claim **público** (`publishable`), a única saída continua sendo **bare metal** com controle de
`cpufreq` — nenhuma quantidade de software resolve isso numa VM.

## Dois perfis estavam mortos por construção, e não era o hardware

Ao perseguir `nightly` apareceram dois defeitos no arnês, e ambos tornavam `nightly` e `release`
inalcançáveis **em qualquer máquina**:

1. **A CLI nunca construía um `IsolationPlan`.** Os dois perfis declaram `isolation_required`, o que
   torna `cpu_limit` e `memory_limit` obrigatórios — e `RunRequest.isolation` ficava sempre no default
   vazio. Não era limitação de máquina: era superfície faltando. Corrigido com `--cpu-set`,
   `--memory` e `--numa-node`.

2. **`apply_isolation` nunca marcava `memory_limit_applied = True`.** Os dois ramos devolviam
   ausência, e um deles aconselhava *"run under an externally created cgroup instead"* — conselho que
   o arnês nunca verificava. Corrigido lendo o limite do cgroup em que o processo já roda: aplicar
   exigiria privilégio e teria efeito colateral sobre o host; ler não tem nenhum dos dois.

E uma armadilha de unidade que custou uma corrida: **`systemd` lê `48G` como 48 GiB** (base 1024) e o
parser lia como 48 GB (base 1000). O mesmo texto ia para os dois lugares significando coisas
diferentes, o cgroup ficava mais frouxo que a declaração, e o portão reprovava — com razão. O parser
passou a seguir systemd e docker no sufixo simples.

# As sete armadilhas — e a regra que torna a lista obsoleta

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

7. **A corrida terminou e o script local ficou pendurado duas horas.** O `bench-run.sh` fechou no
   droplet com `rc=0` às 00:40; o SSH que o conduzia morreu sem devolver, e o `bench-droplet.sh`
   local esperou até alguém olhar. **~US$ 1,50 de host ocioso**, com os resultados prontos lá dentro.

   A lição é uma distinção que eu não tinha feito: **o `trap EXIT` protege contra o script MORRER; ele
   não protege contra o script TRAVAR.** São falhas diferentes e precisam de defesas diferentes —
   `ServerAliveInterval` derruba a sessão quando o outro lado some, e um `timeout` cobre o caso em que
   ele responde e nunca termina.

   ```bash
   timeout "$MEDICAO_TIMEOUT" ssh -o ServerAliveInterval=30 -o ServerAliveCountMax=6 …
   ```

   E o código de saída 124 do `timeout` é tratado como **"pode haver resultado lá"**, não como falha
   limpa: foi exatamente o caso — a medição existia, só a condução tinha travado, e a coleta a trouxe.

# A regra, que vale mais que a lista

Seis das sete têm a mesma forma: **capacidade que existe na máquina de desenvolvimento e não num host
limpo, descoberta DEPOIS do trabalho caro.** A sétima é de outra família e por isso vale destacá-la: não
falta capacidade nenhuma — falta **teto de tempo** numa chamada remota, e a defesa contra travar não é a
mesma que a defesa contra morrer. Uma lista de armadilhas não impede a sétima — ela só
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
