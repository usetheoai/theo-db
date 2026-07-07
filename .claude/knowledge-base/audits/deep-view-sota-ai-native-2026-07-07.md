# Deep-View — TheoDB no caminho do SOTA AI-native database? (System Design + Gaps)

Date: 2026-07-07 · Escopo: avaliação honesta (Regra 3) de trajetória vs North Star (igualar/superar AlloyDB, Opção α, ADR 0002). Evidência: council-vector-ann (pilar vetorial) + auditoria direta de M53/M54/M55 + ADRs/PRD/benchmarks.

## Veredito executivo

**Estamos no caminho ESTRUTURALMENTE certo, mas ainda NÃO no destino.** O ROADMAP V1 (M0–M55) está 100% `[x]` e construiu um banco AI-native **coeso e crash-safe** com **paridade de recall vetorial medida** e uma **superfície AI-native real** (embed, hybrid, NL→SQL, vectorizer declarativo). Mas o **eixo diferenciador do North Star — superioridade vetorial de performance comprovada por benchmark — NÃO está cumprido**, e o único lever construído para fechá-lo (SBQ inline) **não demonstrou mover o asymptote na única escala medida**. Nota global honesta: **~5.5/10** para "SOTA AI-native que supera AlloyDB".

O que vencemos HOJE (real): abertura (Apache 2.0), custo, portabilidade (roda em qualquer Postgres), model-agnostic, e uma superfície AI-native que **iguala pgai/Supabase em capacidade**. O que ainda perdemos: **performance vetorial vs AlloyDB/ScaNN (~25× gap intacto)**, **escala operável (muro do VACUUM)**, e **HA/control-plane (fora do repo — gap de produto vs AlloyDB managed)**.

## Mapa de system design (as camadas)

```
Interface SQL:  ai.*  (embed, hybrid_search[_rrf], nl_to_sql, rank, analyze_sentiment, chat)
                theodb.*  (create_vectorizer, chunk_text, import_vectors, *_knn)
                       │
Orquestração:   hybrid.rs (RRF via SPI) · nl.rs (L1-L4 sandbox) · vectorizer.rs (fila+bgworker)
                       │
Núcleo próprio: am/ (theodb_hnsw / theodb_ivfflat — grafo imutável, SBQ inline, iterative scan,
                       fold crash-safe M48, advisory lock) · ann/ · sbq.rs · pq.rs · vec.rs (SIMD)
                       │
I/O externo:    embed.rs + http.rs (HTTP OpenAI-compat, retry bounded, SSRF) · chat.rs
                       │
Base:           Postgres 17 + pgrx 0.16 (extensão, SEM fork do engine — ADR 0001/0006)
```
Coesão: **boa** — módulos com SRP claro (embed/http/hybrid/nl/vectorizer/am separados), DIP no I/O (http reusado). Sem god-modules. A superfície `ai.*`/`theodb.*` é AlloyDB-shaped.

## Scorecard por dimensão

| Dimensão | Nota | Estado honesto |
|---|---|---|
| **Recall vetorial vs SOTA permissivo (pgvector)** | 8/10 | **Paridade REAL medida** (M45 SIFT1M 1M×128 PARITY; M50 knob-a-knob 0.94≈0.935). Dentro de índice transacional persistente, não lib in-memory. |
| **Latência/QPS vetorial vs pgvector** | 5/10 | Paridade-a-ligeiramente-atrás: pgvector ~1.6× mais rápido 1-cliente, +29% QPS a 8 clientes (M50). |
| **Superioridade vs AlloyDB/ScaNN (o P0 do CTO)** | 2/10 | **Gap ~25× QPS INTACTO** (M33: ScaNN 1920 QPS @0.99 vs theodb 78). NÃO cumprido. |
| **Superfície AI-native (embed/hybrid/NL/vectorizer)** | 7/10 | Rica e **medida**: hybrid BEIR real (M53 scifact: hybrid=vector recall 0.9733, BM25≫ts_rank_cd); vectorizer declarativo com worker crash-safe (M54, e2e verde). Iguala pgai/Supabase em capacidade. |
| **System design / crash-safety** | 7/10 | Coeso, crash-safe (fold M48, fila do vectorizer com fencing/reaper). Bem revisado (council reviews acharam+corrigiram HIGH FFI). |
| **Escala operável / production-worthy** | 3/10 | **Muro do VACUUM (M55): 86s parada total a 100k → ~14min a 1M**; teto de RAM do BUILD; sem disk-resident; bgworker single-DB/single-worker. |
| **Coerência estratégica (D1-D7, North Star)** | 8/10 | Decisões LOCKED e sãs; measurement-first honesto; anti-sunk-cost real (PQ kill, SBQ gated). |

## Os GAPS críticos (priorizados por alavancagem)

### P0 — Superioridade vetorial permanece HIPÓTESE, não resultado
O claim `≥2× QPS do SBQ inline a recall≥0.99` é **UNBENCHMARKED** — gated em escala-com-pressão-de-memória (1M@768d ou ≥250k@1536d) numa box quieta. A 25k o corpus cabe em RAM → a compressão 4× não tem onde ganhar. **Toda a tese do AM próprio (ADR 0015) depende desse número.** Enquanto for UNBENCHMARKED, "superioridade vetorial" viola a Regra 5 do próprio projeto (performance é claim, não opinião) se tratada como cumprida. É o **fork na estrada**.

### P1 — O gap de 25× vs ScaNN é quantização anisotrópica + AH SIMD, não bit-quantization
O edge do ScaNN/AlloyDB é *anisotropic score-aware loss + Asymmetric Hashing SIMD*, não o SBQ Hamming barato. O M39 já nomeou esse lever. Sem ele, o gap absoluto vs AlloyDB não fecha — SBQ é otimização de fator-constante, não de asymptote de recall×QPS.

### P2 — SIMD só no L2; cosine/IP (o caso dos embeddings reais) roda ESCALAR
`vec.rs`: `dot_from_bytes`/`cosine_dist_from_bytes` são escalares; só L2 tem AVX2. Mas embeddings reais (OpenAI/Cohere) são cosine/IP → o hot path deles não tem SIMD. Fator-constante direto no eixo que o M50 aponta como o teto. É um ganho barato e não-colhido.

### P3 — Muro do VACUUM (M55) bloqueia a PRÓPRIA escala onde P0/P1 seriam medidos
Fold O(N) whole-index sob EXCLUSIVE: 86s de parada a 100k, ~14min projetado a 1M, ~14GB RAM (escapa do maintenance_work_mem). ADR 0017 já decidiu o caminho (híbrido tombstone-in-place + fold-para-compaction) mas a **implementação é milestone futuro** — e é pré-requisito de v1.0 E de medir P0 a 1M.

### P4 — Filtered ANN 3× mais lento que pgvector 0.8 (M52)
Paridade de recall, mas re-busca vs resume-from-discarded. Déficit de eficiência estrutural no filtered path (o caso RAG real: `WHERE tenant=X ORDER BY emb`).

### P5 — Gaps de PRODUTO vs AlloyDB managed (fora do repo)
HA/replicação/control-plane não existem no repo (deploy/plataforma). Para competir com AlloyDB **managed**, isso é um gap de produto — mas é uma aposta deliberada (Patroni/control-plane como camada separada, D6). Model garden/endpoint management é mais raso que o AlloyDB (só endpoint configurável via GUC).

### P6 — Superfície AI-native: faltam re-ranking, chunking avançado, RAG patterns
Vectorizer v1 tem chunking de janela-de-caracteres (não recursivo separator-aware); hybrid não tem cross-encoder re-rank; sem retrieval-quality tuning loop. Iguala pgai/Supabase mas não os supera.

## Distância honesta de um V1 defensável

Dado `public-copy.md §3` (sem "production-ready" sem evidência sustentada) e o muro do VACUUM: **não podemos honestamente claimar v1.0/produção hoje.** O `/dogfood` exigiria uso interno sustentado em infra real — que a parada de 14min no VACUUM a 1M torna inviável para um workload de escala. **A implementação da fase 1 do ADR 0017 é o gate honesto de v1.0.**

## Os 3 movimentos de MAIOR alavancagem estratégica

1. **DESTRAVAR + MEDIR o P0** (a aposta que define tudo): implementar a fase 1 do M55 (tombstone-in-place — remove o muro do VACUUM) → habilita medir o claim ≥2× QPS do SBQ inline a 1M@768d numa box dedicada. **Este é o fork na estrada:** materializa → valida o AM próprio + primeiro sinal de superioridade; falha → reabre ADR 0015 e redireciona para P1 (quantização anisotrópica), que é onde o ScaNN ganha os 25×.
2. **Colher os ganhos baratos de latência** (P2): SIMD para cosine/IP (o caso real dos embeddings) + resume-from-discarded no filtered scan (P4). Fatores-constantes no eixo exato onde perdemos para pgvector, sem mudar arquitetura.
3. **Decidir o pilar de PRODUTO** (P5): HA/control-plane é V2 ou é gate de "competir com AlloyDB"? Se o alvo é OSS/on-prem/self-hosted (é), a superioridade estrutural (abertura+custo+portabilidade) já vence — e o foco deve ser **superioridade vetorial provada**, não paridade de plataforma managed.

## Honestidade final

O projeto executou o roadmap com **rigor de medição e anti-sunk-cost exemplares** — é raro ver "performance é claim, não opinião" honrado tão consistentemente (PQ kill, SBQ gated, benchmarks com caveats, reviews adversariais). A arquitetura é sã e evolvível. **O que falta não é mais features — é fechar o UM eixo que o North Star exige (superioridade vetorial) e que ainda é hipótese.** Estamos no caminho certo; falta o último terço, e ele é o mais difícil (o algoritmo, não a plumbing).
