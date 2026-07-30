---
type: Honest Negative
title: Códigos 768× menores co-locados com o f32 no mesmo tuple não reduzem I/O algum
description: O walk pagina os ~3 KB inteiros para ler 4 bytes; separar o layout também não bastou, porque o rerank lê NN-leaves frios enquanto o baseline revisita hub-nodes cacheáveis.
resource: docs/benchmarks/m59-anisotropic-ah.md
tags: [vetorial, quantizacao, layout, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# Códigos 768× menores **co-locados** com o f32 no mesmo tuple não reduzem I/O algum

## O veredito (M59)

Quantização anisotrópica (AQ) com códigos de **4 bytes/vetor** contra f32 de **3072 bytes** — 768× menores. O
ganho de QPS a recall casado, 500k×768d:

| Layout | in-RAM | sob pressão |
|---|---|---|
| **v3** (código co-locado com o f32) | **1,01×** | 1,03× (1,3 GB) |
| **v4** (código em layout separado) | **0,99×** | 1,1× (pressão forte, 700 MB) — ambos colapsam a ~2 QPS |

## Os dois mecanismos, e o segundo é o interessante

1. **v3 — co-locação anula o ganho.** O código de 4 B mora **ao lado** do f32 de 3 KB no mesmo tuple. O walk
   pagina os ~3 KB inteiros para ler os 4 B; o working set quente **nunca encolhe**.
2. **v4 — separar também não bastou.** Sob pressão, o rerank passa a ler os **NN-leaves f32 frios** (pouco
   cacheáveis), enquanto o baseline f32 revisita **hub-nodes** (cache-friendly). A separação troca um padrão de
   acesso bom por um ruim.

> **O ganho de quantização é estrutural do CARRIER, não do quantizador.**

## A confirmação por medição — e ela reabre o eixo

O **mesmo** quantizador (AQ+AH) no carrier **IVF batch-scan** (M75) deu **2,3× a 5,1×** a recall casado — porque
lá as listas são contíguas, as leituras sequenciais e o kernel é batched. O lever era o carrier, e a medição prova
os dois lados.

Ressalva do M75: **n=5000, não 1M**; só a comparação relativa vale nessa escala; in-memory, single-thread, **sem
o imposto de página/WAL** do AM pgrx. E o AVQ train é super-linear (build 23,0 s vs 3,3 s do f32).

## Ressalva sobre o único ponto favorável

O 1,16× a 20k in-RAM é desqualificado pelo próprio artefato: *"1 run / 20 queries, Δp50 ~0,1 ms sub-ms — NÃO é
uma vitória decision-grade"*.

## Relacionados

- [honest-negative/superioridade-vetorial-vs-scann](superioridade-vetorial-vs-scann.md)
- [technique/a-forma-da-curva-diagnostica-a-causa](../techniques/a-forma-da-curva-diagnostica-a-causa.md)
