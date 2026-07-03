---
name: deep-research
version: 0.1.0
requires: []
description: Deep research on a topic — our system (code file:line, ADRs, blueprints, benchmarks) + the SOTA (papers, benchmarks, techniques, calculations) — producing a handbook-quality chapter in the 5-layer pattern (Teoria → Matemática → Nossa implementação → Nosso benchmark → SOTA & gap honesto), curate-not-reproduce, with every citation validated to resolve on disk. Use to write or evolve a handbook chapter, or to build a grounded research dossier before a milestone.
user-invocable: true
allowed-tools: Read Glob Grep Bash Write WebSearch WebFetch Task Skill
argument-hint: "{topic-or-chapter-slug}   e.g. quantization-in-index  |  parte-06-vetorial/20-quantizacao"
---

# Deep Research — pesquisa profunda ancorada, no padrão do handbook

Produz um **capítulo de qualidade-handbook** (ou um dossiê de pesquisa) sobre um tópico, pesquisando **tudo** — o
nosso sistema, papers, benchmarks, cálculos, técnicas do estado da arte — e destilando no padrão de 5 camadas do
handbook, sob o contrato de honestidade. É o motor que transforma "queremos entender X a fundo" num artigo
ancorado, com **zero fabricação**.

> **Fonte de verdade da filosofia:** [`docs/handbook/README.md`](../../../docs/handbook/README.md). Leia-o antes
> de invocar. Esta skill é a máquina que **executa** aquele contrato; o capítulo 19 (HNSW) é o padrão de qualidade
> que todo output desta skill deve alcançar.

## Quando invocar

- Escrever um novo capítulo do handbook (Partes IV–IX, o coração ancorado).
- Aprofundar uma técnica antes de um milestone (ex.: "quantização-no-índice" antes do M36) — o dossiê vira o
  insumo do `/discover-plan` / `/to-plan`.
- Reconciliar os nossos números contra o SOTA em condições casadas (recall, hardware).

NÃO invoque para:
- Um fato pontual que um `grep` resolve (use Grep direto — Regra 9 / loop-engine-convention).
- Reproduzir matemática/history/PG-internals já cobertos por Strang/CLRS/Suzuki — o contrato é **curar, não
  reproduzir** (essas partes viram trilha de leitura anotada, não texto original).

## Filosofia (herdada do handbook — não re-decidir)

- **Curar não reproduzir (Regra 9).** Fundamentos (matemática, história, PG internals) = trilha de leitura às
  fontes canônicas + o "porquê isto importa no TheoDB". Original só onde temos código/benchmark reais (🟢).
- **Contrato de honestidade (Regra 3).** Toda citação de código resolve no disco (`arquivo:linha`); todo número
  de performance vem de um artefato reproduzível OU é marcado `UNBENCHMARKED`; gaps explícitos; aspiracional
  marcado 🟡.
- **Padrão de 5 camadas** (capítulos 🟢): Teoria → Matemática → Nossa implementação (`file:line`) → Nosso
  benchmark → SOTA & gap honesto.
- **Rigor PhD** (herdado de `rules/discover-phd-rigor.md`): ≥ 2 fontes primárias por técnica; SOTA-anchoring
  (posicione contra AlloyDB/ScaNN/pgvector); WebFetch só no `rules/discover-web-allowlist.txt`.

## Fluxo (6 fases)

### Fase 0 — Contrato + escopo
1. Leia `docs/handbook/README.md` (o padrão de 5 camadas + a legenda 🟢/🔵/🟡 + o índice/mapa de fontes).
2. Resolva o alvo: um slug de capítulo (`parte-06-vetorial/20-quantizacao`) ou um tópico livre. Localize a linha
   do índice do handbook que ancora o tópico + o mapa de fontes já declarado ali.
3. **Classifique o tópico:** 🟢 (temos código real → capítulo original), 🔵 (fundamento → curar), 🟡 (roadmap →
   marcar como aspiracional). Se 🔵/🟡, o output é uma trilha de leitura + conexão, NÃO um capítulo original.

### Fase 1 — GROUND (o nosso sistema primeiro)
O que nos torna únicos. Inventarie os artefatos reais do tópico e monte o **mapa de fontes**:
```bash
# código (file:line) — a camada 3
grep -rn "<símbolo-do-tópico>" theodb_rs/src --include=*.rs
# decisões
ls docs/adr/ ; grep -rl "<tópico>" docs/adr/
# pesquisa prévia
ls .claude/knowledge-base/discoveries/blueprints/ | grep -i "<tópico>"
# números
ls docs/benchmarks/*.json
```
Para cada afirmação futura sobre o nosso sistema, registre a âncora `arquivo:linha` que a prova. **Leia** os
arquivos — não opine de memória. (Opcional: invoque o agente do Conselho do domínio — ex.
`Task(subagent_type: "council-vector-ann", ...)` — para o grounding profundo.)

### Fase 2 — RESEARCH (o SOTA: papers, técnicas)
Pesquise o estado da arte **dentro do allowlist** (`rules/discover-web-allowlist.txt` — arxiv, ACM, NeurIPS, ICML,
VLDB, USENIX, IEEE, research.google, github, …):
1. `WebSearch` para o paper seminal + surveys + as técnicas concorrentes.
2. `WebFetch` (só domínios do allowlist) para extrair: a definição, a matemática, a complexidade, e **os números
   do SOTA com as condições** (dataset, recall, hardware — nunca um número solto).
3. **≥ 2 fontes primárias por técnica** (rigor PhD). Um blog não é evidência; um paper + doc oficial/repo mantido
   é. Registre cada citação com URL resolvível.
4. Cruze com os peers já clonados em `.claude/knowledge-base/references/` (pgvector, pgvectorscale, vectorchord,
   duckdb, …) — leitura de código real do SOTA, não só o paper.

### Fase 3 — BENCHMARKS & CÁLCULOS
1. Puxe os **nossos** números dos artefatos (`docs/benchmarks/*.json`) — recall/QPS/p50, com repro + hardware.
2. Calcule a complexidade (Big-O de build e query) e verifique-a contra os dados (a assinatura empírica — ex.:
   pages-read plano em N prova O(ef·M), não wall-clock).
3. **Reconcilie SOTA vs nós em condições CASADAS** (mesmo recall!). Comparar QPS em recalls diferentes é spin —
   delegue a metodologia ao `Task(subagent_type: "council-benchmark", ...)` quando houver dúvida.
4. Onde não temos artefato para um número nosso, marque **`UNBENCHMARKED`** e proponha a medição (reusar
   `benchmarks/theodb_bench/`). NUNCA invente um número.

### Fase 4 — SYNTHESIZE (escrever o capítulo)
Escreva em `docs/handbook/<parte>/<NN>-<slug>.md` usando `templates/chapter-template.md` (as 5 camadas +
exercícios + referências). Regras:
- 🟢: as 5 camadas completas, cada afirmação nossa com `file:line`, cada número com link de benchmark ou
  `UNBENCHMARKED`.
- 🔵: trilha de leitura anotada + a conexão TheoDB ("por que isto importa no nosso código"). NÃO reproduza o
  textbook.
- 🟡: marque aspiracional; descreva o roadmap, não finja implementação.
- **Gap honesto sempre explícito** (onde perdemos pro SOTA, com o número).

### Fase 5 — VALIDATE (o gate de honestidade — fail-closed)
Rode o validador antes de considerar o capítulo pronto:
```bash
python3 .claude/skills/deep-research/scripts/validate_citations.py docs/handbook/<parte>/<NN>-<slug>.md \
  --allowlist .claude/rules/discover-web-allowlist.txt
```
Ele verifica (mecaniza o contrato de honestidade):
- **Toda citação `arquivo:linha` resolve no disco** (o arquivo existe e tem aquela linha). Fabricada → **INVALID**.
- **Toda URL externa está no allowlist.** Fora do allowlist → **INVALID**.
- **Toda afirmação de performance** ("Nx", "QPS", "recall 0.9…") tem um link de benchmark OU o marcador
  `UNBENCHMARKED` no mesmo parágrafo. Número solto → **NEEDS_REVISION**.

Só emita o capítulo quando o validador der **PASS**. Honestidade > completude (Regra 3).

## Gates de honestidade (LOCKED — espelham os golden-rules)

| Gate | Verdict |
|---|---|
| Citação `arquivo:linha` não resolve no disco | **INVALID** |
| URL externa fora do allowlist | **INVALID** |
| Afirmação de performance sem benchmark nem marcador `UNBENCHMARKED` | **NEEDS_REVISION** |
| < 2 fontes primárias para uma técnica na camada SOTA | **NEEDS_REVISION** |
| Reproduz fundamento já coberto por fonte canônica (viola curar-não-reproduzir) | **NEEDS_REVISION** |
| Gap com o SOTA omitido quando existe (ex.: o ~25× vs ScaNN) | **NEEDS_REVISION** |

## Output

- `docs/handbook/<parte>/<NN>-<slug>.md` — o capítulo (ou a trilha curada, se 🔵/🟡).
- `docs/handbook/.research/<slug>-research-<date>.md` — o log: fontes (com URLs resolvíveis), cálculos, o mapa de
  fontes, e o resultado do validador. O rastro de auditoria da pesquisa.
- Atualização da linha do índice em `docs/handbook/README.md` (status do capítulo → ✅ escrito).

## Anti-patterns

- **Reproduzir o textbook** (matemática/internals) em vez de curar. O contrato é claro: 🔵 = trilha de leitura.
- **Número sem artefato.** "~2× mais rápido" sem benchmark ou `UNBENCHMARKED` é o pecado capital (`public-copy.md`).
- **Comparar em recall diferente.** O SOTA-vs-nós é sempre em recall casado; senão é spin (a lição do M35 review).
- **Citação de memória.** Toda âncora `file:line` é lida e validada — a disciplina dos blueprints.
- **WebFetch fora do allowlist.** Fonte não-autoritativa não é citável; estenda o allowlist via ADR se necessário.
- **Escrever 🔵/🟡 como se fosse 🟢.** Fundamento curado e roadmap aspiracional NUNCA se disfarçam de implementação medida.

## Cross-references

- Filosofia (fonte de verdade): `docs/handbook/README.md`
- Padrão de qualidade: `docs/handbook/parte-06-vetorial/19-hnsw.md` (o capítulo-farol)
- Rigor PhD + allowlist: `.claude/rules/discover-phd-rigor.md`, `.claude/rules/discover-web-allowlist.txt`
- Honestidade & copy: `.claude/rules/public-copy.md`, Regra 3 (`~/.claude/CLAUDE.md`)
- Conselho Técnico (grounding de domínio): `docs/conselho-tecnico-theodb.md` — invoque `council-*` para a Fase 1/2/3
- Ciclo irmão (produz blueprint de design, não capítulo): `.claude/rules/cycle-discover.md`
- Template: `templates/chapter-template.md` · Validador: `scripts/validate_citations.py`
