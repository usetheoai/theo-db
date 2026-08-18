---
type: Measurement
title: o teto de escala do build de HNSW é RAM, não disco — ~1,7 GB por milhão, e OOM a 20M
description: CREATE INDEX USING theodb_hnsw sobre 20M vetores foi morto pelo OOM killer com 10 GB de anon-rss. A curva medida (250k → 606 MB, 1M → 1871 MB) projeta ~34 GB a 20M, contra os 16 GB do host — e corrige um dimensionamento que olhava o disco.
resource: theodb-bench @ workspace 2026-08-17
tags: [benchmark, hnsw, escala, memoria, honest-negative, retratacao, b076, b075]
generated: { by: claude-code/opus-5, at: 2026-08-17 }
sources:
  - id: b076
    resource: theo-db issue #230
    title: "#230 — theodb_hnsw: o build materializa o corpus"
    last_modified: 2026-08-17
  - id: scale
    resource: theodb-bench @ workspace 2026-08-17
    title: theodb-bench — escala de referência de 20M
    last_modified: 2026-08-17
---

> **Correção nº 2, por acréscimo (2026-08-17, mais tarde).** A projeção de ~34,7 GB abaixo era uma
> **extrapolação** do ajuste 250k–2M, e a medição direta a 20M numa máquina de 64 GB a **contradiz por
> ~2,6×**: o consumo privado real é **~13,0 GB** (`RssAnon`), ou **0,65 GB por milhão** contra os
> 1,73 GB/milhão do ajuste. **O consumo é sublinear.** O ajuste era bom onde foi ajustado — o terceiro
> ponto o confirmou com 2% de erro — e não onde foi usado; é o risco de extrapolar uma década de escala.
> **O defeito não muda:** 13 GB de memória privada para indexar um corpus de 11 GB continua sendo o
> corpus materializado, continua ignorando `maintenance_work_mem`, e continua matando o backend num host
> de 16 GB (o OOM veio com `anon-rss:10033724kB`, logo abaixo dos 13 GB necessários). Muda só o teto:
> ~15M num host de 16 GB, em vez dos ~4,7M projetados — e ainda assim não foram os 20M.
>
> **Este conceito corrige, por acréscimo, o dimensionamento de escala publicado horas antes.** A conta
> anterior — em [teto de escala do arnês](/benchmarks/harness-scale-ceiling.md) e na recomendação de 20M
> como escala de referência — era de **disco**, e estava certa sobre disco. Ela era **silenciosa sobre
> RAM**, e é a RAM que decide.

**O build de `theodb_hnsw` mantém o corpus em memória privada do backend: ~1,7 GB por milhão de vetores
de 128 dimensões, e `maintenance_work_mem` não o limita.** Medido no droplet efêmero `138.197.22.192`
(s-8vcpu-16gb, nyc3).

# O que aconteceu a 20M

```
Out of memory: Killed process 209232 (postgres) … anon-rss:10033724kB
LOG:  client backend (PID 45071) was terminated by signal 9: Killed
DETAIL:  Failed process was running: CREATE INDEX "bench_vectors_hnsw_l2_idx"
    ON "bench_vectors" USING theodb_hnsw ("embedding" theodb_hnsw_l2_ops) WITH (m = 16)
```

A recuperação do PostgreSQL **funcionou** — `redo done` em 2,98 s, servidor pronto em seguida. O defeito
é o consumo, não a durabilidade.

# A curva

Cada ponto medido isoladamente, `maintenance_work_mem = 64MB`, `m = 16`, pico de `VmRSS` do backend lido
de `/proc/<pid>/status`. **Uma primeira tentativa foi descartada**: três instâncias minhas rodavam em
paralelo e os números (250k → 1076 MB, 500k → 1037 MB) não faziam sentido justamente por isso.

| N | pico VmRSS | tempo | índice em disco |
|---|---|---|---|
| 250 000 | **606 MB** | 84 s | 181 MB |
| 1 000 000 | **1871 MB** | 422 s | 724 MB |
| 2 000 000 | **3629 MB** | 1096 s | 1448 MB |

- Ajuste nos três pontos: **~174 MB fixos + ~1727 MB por milhão**. O terceiro ponto foi previsto pelo
  ajuste dos dois primeiros com **2,0% de erro** — é isso que torna a projeção utilizável, porque não há
  economia de escala esperando adiante.
- ~~Projeção a 20M: **~34,7 GB**~~ — **medido a 20M: ~13,0 GB de `RssAnon`** (0,65 GB/milhão). A projeção
  linear super-estimou em ~2,6×; ver a correção nº 2 no topo.
- Teto deste host com o número **medido** (≈8,3 GB disponíveis): **~13M** — e o host ainda assim não
  suportou 20M, que precisa de ~13,0 GB contra os ~10 GB que ele tinha.
- O índice **em disco** é exatamente linear: **724 MB por milhão nos três pontos, sem desvio**. O custo
  está no build, não no artefato.
- Tempo é super-linear **e piorando**: 4× as linhas custaram **5,0×**, 2× custaram **2,6×**
  (≈N^1,16 e N^1,38).

# Os dois tetos, lado a lado

| | por disco (1,27 GB/milhão) | por RAM (**0,65 GB/milhão**, medido a 20M) |
|---|---|---|
| host medido | 309 GB | 16 GB (≈10 GB úteis) |
| escala comportada | **~200M** | **~13M** (do consumo medido a 20M) |

**A escala de 20M carrega e consulta** — 20 000 000 de linhas, 11 GB de tabela, consultas exatas
respondendo. Ela **não constrói grafo** neste host. As duas coisas são verdade ao mesmo tempo, e reportar
só a primeira seria a omissão que este conceito existe para impedir.

# A causa, localizada

`am/build.rs:403`, em `ambuild_hnsw`, chama `collect_corpus` **incondicionalmente**. A rota de memória
limitada existe no repositório (M96, `build_stream.rs::should_stream`, que já compara `N × dim × 4` contra
o GUC certo) e está atrás do gate `pq_subspaces > 0 && separate_storage` — opções do **IVFFlat**. O
caminho hnsw nunca a consulta.

# Por que isso não é um bug isolado

A issue [#221](https://github.com/usetheoai/theo-db/issues/221) registra a **mesma classe** no colunar:
`flush_pending` consumindo ~7× o `maintenance_work_mem`, OOM do backend a 100M. Dois componentes
independentes ignorando o mesmo orçamento é **ausência de contrato de memória no projeto**, e tratá-los
como dois bugs separados perde exatamente essa leitura.

# O que isto obriga a dizer

Enquanto durar, nenhuma alegação de escala do pilar vetorial pode citar disco sem citar RAM. O
[North Star](/decisions/0002-north-star-equal-or-superior-to-alloydb.md) fala em escala billion; o teto
medido de construção neste host é de **milhões**, e a diferença não é de grau.
