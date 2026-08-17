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

- Ajuste: **~184 MB fixos + ~1687 MB por milhão**.
- Projeção a 20M: **~34 GB**, contra 16 GB — coerente com a morte aos 10 GB (não chegou a pedir tudo).
- O índice **em disco** é exatamente linear (4× linhas → 4× tamanho). O problema é o build, não o artefato.
- Tempo é super-linear: 4× as linhas custaram **5,0×** (~N^1,16).

# Os dois tetos, lado a lado

| | por disco (1,27 GB/milhão) | por RAM (1,69 GB/milhão) |
|---|---|---|
| host medido | 309 GB | 16 GB (≈10 GB úteis) |
| escala comportada | **~200M** | **~5,8M** |

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
