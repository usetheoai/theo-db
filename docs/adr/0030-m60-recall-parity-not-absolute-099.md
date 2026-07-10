# ADR-0030 — M60: DoD do recall vira PARIDADE-pgvector (não 0.99 absoluto), fechado pelo caminho SBQ

- **Status:** Accepted (2026-07-10)
- **Milestone:** M60 (Roadmap v5 — Superioridade vetorial P0)
- **Decisão do owner:** 2026-07-10 (opção A — reenquadrar a DoD e fechar M60 via SBQ; opção B — investigação f32 profunda — autorizada como follow-up).
- **Supersede:** o critério `recall@10 ≥ 0.99 a 500k×768d` do bloco M60 do `ROADMAP.md`.

## Contexto (medido)

O M60 nasceu (spun-off do M57) com a DoD `recall@10 ≥ 0.99 a 500k×768d`, sob a premissa de que o `theodb_hnsw`
tinha um gap de recall **específico** de ~2–3pt vs pgvector, e que 0.99 era alcançável. A medição head-to-head no
**mesmo corpus gaussian-mixture a 500k×768d** (droplet c-8, pg17 — `docs/benchmarks/m60-hnsw-recall.md`,
`docs/benchmarks/m60-raw/`) refuta a premissa:

| Índice (ef=1000, 500k×768d, mesmo corpus, GT exato) | recall@10 |
|---|---|
| pgvector hnsw (m=16, efc=64) | **0.988** |
| theodb_hnsw **SBQ** (over_fetch=32, rerank) | **0.986** |
| theodb_hnsw f32 | 0.974 |

Dois fatos:
1. **O gate 0.99 é um artefato do dado** — *o próprio pgvector só chega a 0.988* (256 clusters gaussianos apertados
   em 768d ⇒ muitos 10-vizinhos quase-equidistantes ⇒ teto de recall@10 < 0.99 para índices da classe HNSW).
   Perseguir 0.99 absoluto é perseguir um número que o SOTA permissivo não atinge nesta distribuição.
2. **O caminho SBQ do theodb já está em PARIDADE com o pgvector** (0.986 vs 0.988 — dentro do ruído de 1 slot de GT
   sobre 500 slots). O caminho f32 puro fica ~1.4pt atrás.

## Decisão

1. **A DoD de recall do M60 passa a ser PARIDADE com o pgvector** (o oráculo de controle), medida no mesmo corpus,
   e **não** o valor absoluto 0.99 (empiricamente inalcançável até pelo pgvector aqui). Isto alinha com a moldura
   **recall-parity** que o projeto já usa como North Star (ADR-0002, measurement-first).
2. **M60 é fechado pelo caminho SBQ** (recall@10 = 0.986 ≈ pgvector 0.988 = paridade medida), com este ADR + a
   evidência em `docs/benchmarks/m60-hnsw-recall.md` como o artefato do milestone.
3. **O gap ~1.4pt do caminho f32 puro fica registrado como follow-up autorizado (opção B)** — NÃO bloqueia M60 nem
   M71–M74. Cinco levers de recall já foram refutados por medição (efc↑, MERGE back-links, m↑, descida-beam ef=1,
   multi-entry `ep←W`); a causa do resíduo f32 é um detalhe de implementação sutil que exige investigação profunda
   e incerta (multi-ciclo). Ver `docs/benchmarks/m60-hnsw-recall.md § levers refutados`.

## Alternativas rejeitadas

- **(rejeitada) Manter a DoD 0.99 e continuar caçando o gap f32.** É perseguir um alvo que o pgvector não atinge;
  5 levers já caíram; violaria o measurement-first (perseguir número, não paridade medida). Anti-sunk-cost (D3).
- **(rejeitada) Marcar M60 done sem ADR/evidência.** Seria fabricar conclusão (Regra 3). Este ADR + os artefatos
  medidos são a evidência.
- **(adiada, autorizada) Investigação f32 profunda (opção B).** Legítima, mas incerta e multi-ciclo; não bloqueia
  o v5. Vira follow-up.

## Consequências

- **Positivas:** M60 fecha com evidência medida e honesta (paridade SBQ↔pgvector); o v5 destrava (M71–M74). A DoD
  passa a ser um alvo alcançável e comparável (paridade), não um artefato.
- **Honestas (trade-off):** a paridade de recall é do caminho **SBQ**, que tem custo de QPS vs f32 (M57 D3 —
  `docs/adr/0018`). Portanto M60 entrega **recall-paridade**, não superioridade de recall nem de latência — a
  latência/QPS é o escopo do M71 (que herda o achado medido: o grafo multi-entry rende +29% QPS a recall igual).
- **Follow-up:** opção B (fechar o ~1.4pt do f32) permanece autorizada e rastreada.

## Cross-references

- Evidência: `docs/benchmarks/m60-hnsw-recall.md`, `docs/benchmarks/m60-raw/*.json`
- Blueprint (discover): `.claude/knowledge-base/discoveries/blueprints/m60-hnsw-recall-quality-blueprint.md`
- North Star / measurement-first: `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`
- SBQ D3 (custo de QPS): `docs/adr/0018-m57-sbq-inline-not-superior.md`
- Roadmap: `ROADMAP.md § M60`, `ROADMAP-v5.md`
