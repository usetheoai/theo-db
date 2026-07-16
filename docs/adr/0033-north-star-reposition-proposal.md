# ADR-0033 — Reposicionar o North Star vetorial: paridade + memória, não superioridade de QPS

- **Status:** **ACCEPTED** (2026-07-16) — assinado pelo owner (paulohenriquevn) via o goal do M104 (system-design
  hardening), que autorizou explicitamente "assinar 0033 OU nota de supersede em 0002". Amenda o LOCKED
  `docs/adr/0002` per o protocolo de mudança de golden-rule (`cycle-rule-schema.md § Golden Rule Change Protocol`);
  a nota de supersede correspondente foi adicionada ao ADR-0002. Fecha a única finding com `rationale_valid=0` da
  auditoria de system-design (`system-design-output/final_report.md`, dimensão Trade-offs).
- **Autor da proposta:** deep research (evidência medida consolidada em `docs/benchmarks/vector-pillar-verdict-2026-07.md`,
  `docs/adr/0035-m73-northstar-vector-verdict.md`, `docs/adr/0036-m74-rabitq-conditional-lever-verdict.md`).

## Contexto (medido)

O ADR-0002 (North Star, Opção α) manda **igualar OU superar o AlloyDB**, buscando **superioridade de QPS vetorial
comprovada por benchmark**. Após perseguir isso por todos os caminhos honestos e medir cada um:

- **Gap 1 (theodb→pgvector):** 7 levers de navegabilidade refutados por medição; o gap é fechável para **paridade**
  (não superioridade). Fix candidato identificado via prior-art (select-neighbors heuristic).
- **Gap 2 (pgvector→ScaNN):** o melhor quantizador permissivo do SOTA (RaBitQ) foi vendorizado e medido a 1M:
  **competitivo com full-precision (8.2ms @ 98.4%), NÃO 25× mais rápido.** O ganho do RaBitQ é **memória** (5.3MB
  residentes @ 98.4% na variante disk), não QPS. O 25× do ScaNN é do algoritmo dele (AH-LUT anisotrópico, 128d) +
  o fato de não pagar o imposto do Postgres que qualquer extensão paga.

## Decisão proposta

**Reescrever a meta do pilar vetorial** de:
> "superar o AlloyDB no vetor (superioridade de QPS por benchmark)"

para:
> **"Paridade vetorial classe-pgvector (recall + latência) + eficiência de memória RaBitQ para billion-scale em
> hardware barato + diferenciação por AI-native/HTAP/abertura/portabilidade."**

Consequências concretas:
1. **Gap 1 vira um milestone de PARIDADE** (fechar o select_from → recall/latência ≈ pgvector), com veredito
   honesto de paridade (não superioridade).
2. **RaBitQ (core vendorizado, ADR-0032) vira uma feature de MEMÓRIA/ESCALA** (índice IVF/MSTG-RaBitQ: billion-scale
   em SSD barato, 32× compressão, latência competitiva) — posicionada como "escala/custo", nunca "mais rápido que o
   AlloyDB". + continuous recall measurement (design VectorChord, estende M67/M68).
3. **O claim público** (`public-copy.md`) passa a ser: "capacidades classe-AlloyDB, abertas + portáveis +
   eficientes em memória", NÃO "vetorialmente superior ao AlloyDB". Honestidade Regra 3/5.
4. **M72/M73** (QPS 1M / head-to-head ScaNN) são reenquadrados como **medições de posicionamento** (documentar onde
   estamos: paridade com pgvector; gap vs ScaNN é estrutural/algorítmico), não como gates de superioridade.
   **M74** vira o ship honesto do RaBitQ-memória.

## Alternativas rejeitadas

- **Manter a meta de superioridade de QPS:** empiricamente inalcançável como extensão PG permissiva (medido). Manter
  seria perseguir um número que a régua não dá — anti-measurement-first, e viraria claim desonesto (`public-copy.md`).
- **Ir para engine standalone (fora do Postgres) pra fugir do imposto PG:** reabre D1/D2/D6 (wire-compat PG é gate),
  é outra categoria de produto — fora do escopo sem novo mandato do CTO.
- **A aposta ScaNN-AH do zero:** possível patente (loss anisotrópico) + anos de tuning; RaBitQ (permissivo,
  training-free) já foi medido e não fecha o QPS-gap no nosso regime.

## O que NÃO muda

- Measurement-first, D1 (licenças), Regra 9, engine Postgres mantido, honestidade. Este reposicionamento é
  *aplicação* dessas regras à evidência, não exceção.
- A Opção α ("igualar" o AlloyDB em capacidades OSS) permanece — só se remove a parte "**superar** no QPS vetorial",
  que a medição refutou.

## Cross-references

- Evidência consolidada: `docs/benchmarks/vector-pillar-verdict-2026-07.md`
- North Star atual (LOCKED): `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`
- RaBitQ vendor: `docs/adr/0032-vendor-rabitq-rs-core.md`
- Root blocker (navegabilidade): `docs/benchmarks/p0-vector-superiority-root-blocker.md`
- Reenquadramentos precedentes (measurement-first): ADR-0030 (M60 recall-paridade), ADR-0031 (M71 latência-melhoria)
- Copy honesto: `.claude/rules/public-copy.md`
