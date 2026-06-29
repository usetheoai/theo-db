# ADR 0002 — North Star: TheoDB igual ou superior ao AlloyDB (Opção α)

**Status:** Accepted (LOCKED) · **⚠️ Superseded in part by [ADR 0006](0006-own-code-postgres-based-rust-go.md) (2026-06-29)** — "compor > construir" deu lugar a "construir código próprio em Rust/Go" (measurement-first preservado) · **Data:** 2026-06-27 · **Owner:** CTO (paulohenriquevn)
**Supersede:** — · **Relacionado:** ADR `0001-no-engine-fork`, PRD §15 (D1–D7), `ROADMAP.md`, blueprint `alloydb-vector-ai-implementation`

> Esta é a **fonte de verdade da estratégia de produto**. Mudá-la exige sign-off explícito do CTO +
> nota de supersede (mesmo padrão de lock das golden rules). Todos os demais documentos resumem e
> apontam para cá (DRY).

## Contexto

Mandato do CTO: **"não importa o esforço ou a complexidade — quero um banco igual ou superior ao AlloyDB"**.
A discovery `alloydb-vector-ai-implementation` (verdict SHIPPABLE 98.7) reconstruiu como o AlloyDB
implementa o motor vetorial/IA e mapeou os gaps OSS. A confirmação pilar-a-pilar mostrou uma verdade
honesta (Regra 3): a estratégia *measurement-first* é **necessária e correta**, mas "igual ou superior"
*literal em cada interno* esbarra num teto que **esforço não fura — licença**: as peças que igualariam o
columnar **in-memory** do AlloyDB (Citus/Hydra/ParadeDB) são **AGPL → barradas pela D1**; e a HA por
**storage desagregado** do AlloyDB é arquitetura de nuvem distinta da nossa (Patroni/pgBackRest).

## Decisão — Opção α

**TheoDB busca ser igual ou superior ao AlloyDB para os seus usuários-alvo (OSS / on-prem / edge /
model-agnostic), assim definido:**

1. **Paridade de capacidades e resultados** nos pilares onde o AlloyDB compete (vetorial/IA, analytics,
   operação) — entregar o mesmo *resultado* ao usuário, com peças permissivas.
2. **Superioridade estrutural — já hoje, sem benchmark:** abertura (Apache-2.0, auditável), custo (sem
   licença por vCPU), portabilidade (mesma imagem laptop→bare-metal), **independência de modelo**
   (qualquer modelo local/remoto vs. lock-in Gemini).
3. **Superioridade de performance no pilar vetorial (killer):** perseguida e **comprovada por benchmark
   reproduzível** (`docs/benchmarks/`), nunca afirmada sem evidência (`public-copy.md`, Regra 5, PRD D3).

### Doutrina operacional (LOCKED)

- **Measurement-first.** O **harness de recall@k + latência/QPS/build/memória reproduzível** é o **1º item
  de M2** e pré-requisito de qualquer claim de performance e do gatilho de fork. Hoje não existe (nem nos
  análogos, nem alcançável no AlloyDB — tudo `UNBENCHMARKED`). Construí-lo é a maior alavanca do programa.
- **Fork é condicional (D3).** Não forkamos `pgvector`/`pgvectorscale` antes do benchmark de gatilho.
  Forkar antes de medir é a complexidade acidental / sunk-cost que o `CLAUDE.md` proíbe.
- **Rota de superioridade no índice:** o **algoritmo ScaNN é Apache-2.0** (só a integração do AlloyDB é
  fechada). As apostas, decididas pelo benchmark da Fase 2, são: adotar pgvectorscale as-is → forkar para
  fechar gap → **ScaNN-as-PG-AM** (trazer o mesmo núcleo SOTA para o Postgres). Esforço alto liberado; o
  *como* é o essencial mais simples que a evidência justificar.
- **Camada de IA (M7):** hybrid search (FTS+vector+RRF) é puro OSS → win imediato; `theodb_ml`
  (embeddings/`ai.*` sobre modelo configurável) substitui o `google_ml_integration` fechado; `ai.rank`
  (rerank) e `theodb_ai_nl` (NL→SQL com guarda anti-prompt-injection) são build novo; **BM25 permissivo**
  é gap real (pg_search é AGPL → barrado) e ganha discovery própria.
- **Esforço ≠ Complexidade.** Esforço ALTO é bem-vindo (ScaNN-AM, fork com CI de rebase, suíte de
  benchmark). Complexidade desnecessária é proibida sempre. Esforço nunca justifica claim sem benchmark.

## Postura de paridade por pilar (resumo da confirmação)

| Pilar | Postura | Observação |
|---|---|---|
| P1 Compat · P6 Segurança · P8 Deploy · P9 Migração | **Paridade alcançável** | M1/M3/M5 |
| P2 Vetorial/IA (killer) | **Paridade/superioridade vencível** | gated em benchmark; ScaNN é Apache-2.0; model-agnostic supera lock-in |
| Abertura · custo · portabilidade · model-agnostic | **Superior hoje** | estrutural, OSS |
| P3 Columnar/HTAP | **Aposta diferente, competitiva** | lakehouse DuckDB (D2), **não** in-memory — forçado por D1 (AGPL barrado). Pode superar em scan grande; difere em HTAP quente |
| P4 HA/DR | **Aposta diferente, competitiva** | Patroni+pgBackRest (M4), não storage desagregado |
| Managed control plane | **Fora do v1 (D7)** | porta aberta via operador K8s |

## O que o esforço NÃO compra (teto de licença) — Opção β fora de escopo

Igualar *literalmente* o columnar in-memory e o storage desagregado exigiria **reabrir D1** (aceitar AGPL —
envenena o Apache-2.0) ou **construir esses componentes do zero** (programa multi-ano). Isso é a **Opção β**
e está **fora de escopo até um futuro ADR supersedendo D1/D2/D7**, assinado pelo CTO. O mandato de
"esforço sem limite" não dissolve a restrição de licença — só esforço não torna AGPL seguro na distribuição.

## Honestidade (LOCKED)

- "Igual ou superior ao AlloyDB" aparece em docs públicos como **missão/meta**, nunca como claim de
  performance não-qualificado (`public-copy.md`). Superioridade de performance só é afirmada **com benchmark
  reproduzível publicado**. O gate de `/review` e o lint de public-copy bloqueiam o contrário.

## Consequências

- O próximo trabalho técnico é o **harness de benchmark** (Fase 0), não um fork nem um componente novo.
- O gatilho de fork D3 passa a ter um caminho de evidência objetivo.
- A divergência honesta em columnar/HA fica registrada — evita que alguém prometa "in-memory como o
  AlloyDB" e que um revisor a trate como bug em vez de decisão.
- Reabrir a paridade literal (β) é uma decisão consciente de licença/arquitetura, não um detalhe de roadmap.

## Quando esta ADR pode mudar

Só com sign-off do CTO + nota de supersede + entrada no `CHANGELOG.md`. Promover a Opção β (paridade
interna literal) exige ADRs específicos supersedendo D1/D2/D7.
