---
type: Index
title: Medições
description: Índice dos conceitos do tipo `Measurement` deste bundle.
tags: [okf, indice]
timestamp: 2026-07-30T00:00:00Z
---

# Medições

Números que **já foram medidos**, com a metodologia e o artefato que os sustentam. Consultar antes de medir de
novo — e antes de afirmar qualquer coisa que um destes já responde.

Toda entrada aqui distingue o que foi **observado** do que foi **calculado**, e marca `UNBENCHMARKED` o que
continua hipótese.

| Conceito | O que é |
|---|---|
| [flush_pending consome ≈ maintenance_work_mem × 8](amplificacao-maintenance-work-mem.md) | Medido por OOM real: mwm=2GB produziu 23,4 GB de anon-rss; mwm=128MB completou a carga de 100M. A fórmula dá a ordem de grandeza e SUBESTIMA o observado em 36% (base: previsto, unidades uniformizadas em GiB). |
| [No scan vetorial o custo é I/O (~50%) e sort (~36%) — a distância f32 é ~15%](custo-do-scan-vetorial-nao-e-a-distancia.md) | Medido com profiler em 200k×128, estável em 5 runs e 3 pontos de probes; falsificou a premissa do M36 e reescopou o milestone para o gargalo real. |
| [M169 medido: ClickBench 100M de 28/43 para 30/43 — que na verdade é 28 pelo streaming + 2 pelo recuo](delta-medido-m169-28-para-30.md) | Mesma box, mesmo corpus (99.997.497 linhas), `so_md5` como única variável. As 4 falhas roteadas caem. A regressão q08/q09 (EMFILE no spill) foi corrigida, mas elas completam pelo RECUO ao eager — consumo O(N), o oposto do objetivo. q32 passa com 1,5% de margem. Byte-identidade provada 4/4 contra o gêmeo heap. |
| [O mesmo binário deu −0,6% e +2,3% em coletas diferentes da mesma box](deriva-de-box-m168.md) | Controle de deriva do M168: reconstruir o binário antigo e rodá-lo intercalado com o novo fechou a pergunta por experimento — a diferença entre coletas era da box. |
| [`GROUP BY` de ~10⁸ grupos SEM `LIMIT` consome 19,5 GB e o kernel MATA o backend](groupby-sem-limit-a-100m-grupos-mata-o-backend.md) | A MESMA consulta completa em 295,6 s com `LIMIT 10`. O discriminador é o RESULT SET que volta ao cliente, não o estado do agregado — os dois casos constroem os mesmos grupos. Está FORA da pool que o streaming limita. |
| [Gap medido vs ClickHouse no ClickBench: 19,4× geral, 7,54× na classe coberta, 303× na não-coberta](gap-vs-clickhouse-m159.md) | Mesma box. O landscape publicado situa o resultado, e o deep-dive identificou a ponte de decode como gargalo da classe coberta. |
| [O júri adversarial descartou 11 de 18 achados — precision 0.39](juri-adversarial-precision-039.md) | Medida da precisão de um review multi-agente: a maioria dos descartes era convenção deliberada lida como defeito. |
| [A 100M o modo de falha deixa de ser lentidão e vira NÃO-CONCLUSÃO — e a taxa depende do REGIME de memória](limite-de-escala-100m-nao-conclusao.md) | Medido duas vezes, 19/43 (box 15 GB, corpus maior que a RAM) e 28/43 (box 31 GB, corpus em page cache). Os dois números NÃO são comparáveis entre si; a classe vale nos dois. |
| [O pushdown agregado NÃO é a regressão de memória do q17 a 100M](q17-pushdown-nao-e-regressao.md) | Isolado em box ociosa, com RSS do backend amostrado durante: 4,58 GB com pushdown ON vs 4,57 GB OFF. O OOM de 12,3 GB vinha do oráculo do harness. |
| [O plano do scan colunar é O(N) — 48,1 MiB a 100M, para QUALQUER projeção](scanplan-e-on.md) | plan_columnar_scan desserializa a grade inteira do diretório (n_chunk_groups × natts), não a largura da projeção — então uma consulta de 1 coluna paga o mesmo que SELECT *. |
| [Offsets i32 do Arrow estouram acima de 21,5 B/linha sobre 100M](teto-offsets-i32.md) | DataType::Utf8 usa offsets i32 (2 GB por array); TRÊS consultas do ClickBench estouram (q20, q33, q34), todas sobre URL, com ordens de grandeza de folga — e é panic, não Result. |
| [Há TRÊS contadores chamados cobertura no ClickBench, e eles não se contradizem](tres-contadores-de-cobertura-clickbench.md) | 35/43 (pushdown sob GUC default), 31/43 (só agg) e os intermediários históricos medem coisas diferentes — confundi-los é a leitura errada mais provável. |
| [Consumir por chunk-group NÃO mudou resultado algum — float8 bit a bit, e 35/35 do espaço de tipos](streaming-nao-muda-sum-float8.md) | Medido 2026-07-31 no M169: eager e streaming deram sum=2.00000000000001e+17 e avg=8000000000000.04 idênticos, com 0.1 não-representável + 1e17 esparso em 3 chunk-groups. Uma forma medida, não uma prova para toda entrada. |
| [count(*) sobre 100M colunar — 11,4 s com o pushdown agregado, >948 s sem ele](count-star-colunar-100m-com-e-sem-pushdown.md) | Medido 2026-07-31 na box de bench (16 vCPU / 31 GB, corpus em page cache). O caminho sem pushdown fica a 99,9% de CPU com zero wait events — é materialização linha a linha, não I/O. Diferença ≥80×. |
| [Materializar o gêmeo heap de 100M custa 1796 s de COPY + 1561 s de SET LOGGED — e o segundo passo não é opcional](custo-de-materializar-o-gemeo-heap-100m.md) | Medido 2026-07-31 na box de bench. O rewrite nunca havia sido cronometrado. Custa quase tanto quanto a carga, e um crash horas depois provou que pulá-lo teria apagado as duas coisas. |
| [O pico da pool no GROUP BY é LINEAR na cardinalidade — 2M grupos usam 95,4% de uma pool de 192 MiB](pico-do-groupby-e-linear-na-cardinalidade.md) | Medido 2026-07-31 sobre 2M linhas com work_mem 64 MB. 10³ grupos → 0,2 MiB; 10⁶ → 91,7 MiB; 2×10⁶ → 183 MiB. É o termo que o streaming NÃO reduz. |
