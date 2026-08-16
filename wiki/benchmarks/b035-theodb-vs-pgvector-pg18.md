---
type: Measurement
title: b035 — TheoDB × pgvector no VectorDBBench, mesmo PG18, recall casado
description: Primeira corrida do arnês multi-sistema. A recall casado (~0,983) o pgvector faz +16% de QPS; a leitura ingênua do mesmo ef_search diria o contrário, e estaria errada.
tags: [benchmark, pgvector, vectordbbench, recall, honest-negative, b035]
item: B-035
generated: { by: claude-code/opus-5, at: 2026-08-12T23:45:00Z }
sources:
  - id: run
    resource: benchmarks/vectordbbench/results/
    title: logs e JSON brutos da corrida
    last_modified: 2026-08-12
---

# O que foi medido

Primeira corrida do **VectorDBBench** com o cliente `theodb`, contra o **pgvector 0.8.6**, os dois sobre
**PostgreSQL 18.4** — igualdade verificada por gate antes de medir, porque o compose do arnês upstream fixa
`pg16` e o TheoDB é PG18-only; comparar assim mediria a versão do PostgreSQL e chamaria de índice.

| | |
|---|---|
| Caso | `Performance1536D50K` — OpenAI, 50.000 vetores, **1536 dim**, métrica **COSINE** |
| k | 10 |
| Máquina | droplet DigitalOcean `g-16vcpu-64gb`, nyc3, **IP 164.90.141.31** (efêmero, destruído ao fim) |
| CPU | Intel Xeon Platinum 8168 @ 2.70 GHz, 16 vCPU, 62 GB, kernel 5.15.0-186 |
| Por que esta máquina | `16c64g` é o rótulo de referência do próprio upstream — 317 ocorrências no repositório e o exemplo da docstring de `db_label` |
| Concorrência | 1, 5, 10, 20, 30, 40, 60, 80 clientes; 30 s por nível |
| Build | `m=16`, `ef_construction=64` nos dois — são os defaults do pgvector e as constantes do TheoDB (`am/build.rs:22-23`), então coincidem sem ajuste |

# Resultado

| Motor | `ef_search` | recall@10 | QPS (pico) | p99 serial | insert | build do índice |
|---|---|---|---|---|---|---|
| pgvector 0.8.6 | 64 | **0,9835** | **3.590,6** | 4,3 ms | 18,8 s | **35,1 s** |
| TheoDB | 64 | 0,9600 | 4.536,8 | 3,8 ms | 19,7 s | 125,1 s |
| **TheoDB** | **128** | **0,9829** | **3.086,1** | 5,2 ms | — | — |
| TheoDB | 256 | 0,9936 | 1.887,7 | 8,1 ms | — | — |

**A recall casado — 0,9829 contra 0,9835 — o pgvector faz +16,3% de QPS** (3.590,6 vs 3.086,1).

**No build do índice o pgvector é 3,6× mais rápido** — e a primeira redação desta linha estava imprecisa.

> **Correção por acréscimo, 2026-08-13.** Publiquei "2,7× mais rápido (53,9 s vs 144,8 s)" citando
> `load_duration`, que **soma inserção e construção**. Decompondo o mesmo JSON:
>
> | | insert (COPY) | build do índice | load (soma) |
> |---|---|---|---|
> | pgvector | 18,80 s | **35,09 s** | 53,88 s |
> | TheoDB | 19,66 s | **125,09 s** | 144,75 s |
> | TheoDB (2ª corrida) | 19,34 s | 122,85 s | 142,18 s |
>
> A inserção está em **paridade** (4% de diferença — o caminho de fio e o armazenamento são comparáveis).
> O que é lento é a **construção do grafo HNSW: 3,6×**, não 2,7×. A redação anterior atribuía à carga um
> custo que é do build, e diluía o tamanho real da diferença.

# A armadilha que esta corrida documenta

A primeira linha da tabela, lida sozinha, diz que o TheoDB faz **+26% de QPS**. É verdade e é enganoso: os
dois estavam em `ef_search=64`, e nesse ponto o TheoDB entrega **recall 0,96 contra 0,9835**. Ele é mais
rápido porque está procurando menos.

Comparar QPS sem casar recall é o erro que a Regra 5 do projeto existe para barrar, e ele aparece aqui na
forma mais tentadora possível: a configuração "igual dos dois lados" (`ef_search=64` em ambos) **parece** a
comparação justa, e não é. O parâmetro é igual; o ponto de operação não.

Foi preciso varrer `ef_search` no TheoDB para achar o ponto comparável — e ali o sinal inverte.

# Reprodutibilidade

Duas corridas independentes do TheoDB em `ef_search=64`, com carga e build de índice refeitos do zero:

| corrida | recall | QPS | carga |
|---|---|---|---|
| 1 | 0,9594 | 4.598,0 | 17,60 s |
| 2 | 0,9600 | 4.536,8 | 17,87 s |

**1,3% de diferença no QPS, 0,06% no recall.** Não é um teste de significância — são duas amostras —, mas é
o bastante para dizer que a diferença de 16% ao pgvector não é ruído desta magnitude.

# Relação com o M72, dita porque parece contradição e não é

O [`m72-qps-multiclient`](m72-qps-multiclient.md) mede **+11% de QPS para o índice próprio** a recall casado.
Esta corrida mede **−16%**. Os dois estão certos, em regimes diferentes:

| | M72 | B-035 (esta) |
|---|---|---|
| Escala | 1.000.000 vetores | 50.000 |
| Dimensão | 128 | **1536** |
| Recall casado em | ~0,91 | **~0,983** |
| Clientes | 8 | até 80 |

O próprio M72 declara medir "um regime declaradamente favorável a ele". Esta medição não o refuta — mostra
que **o resultado não generaliza** para 1536 dimensões, 50 mil vetores e recall alto. Qual dos dois regimes
importa mais depende da carga real, e nenhuma das duas corridas responde isso.

O que **não** muda: o veredito do [`m73-headtohead-verdict`](m73-headtohead-verdict.md) — paridade de recall
alcançada, superioridade de QPS não alegável — segue de pé, e esta corrida o reforça em vez de o abalar.

# O que esta corrida NÃO cobre

- **Sem teste de significância pareada.** O arnês não tem. O `theodb_bench` removido tinha (randomização
  pareada de Smucker/Allan/Carterette). Qualquer alegação comparativa precisa dela por cima; duas amostras
  concordantes não são um teste.
- **Escala pequena.** 50.000 vetores é o menor caso padrão do arnês. A 50 mil, boa parte do custo é cliente
  Python e round-trip, não índice.
- **Um único ponto do pgvector.** Varri `ef_search` só do lado do TheoDB. Uma curva completa dos dois lados
  daria a fronteira de Pareto; esta corrida dá um ponto casado.
- **Sem varredura de `m`/`ef_construction`.** O TheoDB os fixa e o cliente **recusa** qualquer outro valor
  ([[B-036]]). O eixo de qualidade de grafo é inalcançável hoje.
- **Sem `ivfflat`** ([[B-037]]) e **sem `halfvec`** ([[B-038]]).
- **Sem filtro.** O cliente declara só `NonFilter`.
- **Um `db_label` do pgvector saiu com tudo zero** — foi a corrida que falhou por `vector type not found`
  antes do conserto. Está nos artefatos brutos e **não** entra em nenhuma tabela aqui.

# Política que esta corrida ajudou a fixar

O [ADR-0061](../decisions/0061-benchmark-oficial-por-pilar.md) tornou obrigatório o que esta corrida fez por
disciplina: arnês de terceiros, concorrente na mesma máquina, qualidade ao lado de velocidade e ponto de
operação casado.

# Reproduzir

```bash
docker compose -f benchmarks/vectordbbench/docker-compose.yml up -d
uv venv --python 3.11 /tmp/vdbb && . /tmp/vdbb/bin/activate
uv pip install "vectordb-bench[theodb] @ git+https://github.com/usetheoai/VectorDBBench@theodb"
CASE=Performance1536D50K K=10 EF_SEARCH=64 ./benchmarks/vectordbbench/run.sh
```

Logs e JSON brutos em `benchmarks/vectordbbench/results/`; a spec da máquina em `results/machine.txt`.
