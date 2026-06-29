# ADR 0006 — Virada estratégica: banco Postgres-based com código próprio em Rust/Go

**Status:** Accepted (LOCKED — virada de mandato; sign-off CTO 2026-06-29 + 3 decisões de escopo travadas) · **Data:** 2026-06-29 · **Owner:** CTO (paulohenriquevn)
**Supersede (em parte):** ADR `0002` (composição > construção), ADR `0004` (NO-FORK de índice), ADR `0005` (moat = só produto, código próprio mínimo) ·
**Mantém:** ADR `0001` (núcleo: engine PostgreSQL não-reescrito / wire-compat) · ADR `0003` (licença BM25) ·
**Relacionado:** PRD §15 (D1–D7), `ROADMAP.md` → `ROADMAP-v2`

> Mudar um ADR LOCKED exige sign-off do CTO + nota de supersede + CHANGELOG (protocolo `cycle-rule-schema.md`).
> Esta ADR é a fonte de verdade da **nova** estratégia de produto. Os ADRs supersedidos recebem nota
> apontando para cá.

## Contexto

**Mudança de mandato do CTO (2026-06-29), sob pressão de investidores:** o GOTO deixa de ser "distribuição que
compõe peças OSS com uma camada fina" e passa a ser **um banco de dados competitivo, de marca própria, baseado
na engine PostgreSQL (modelo AlloyDB/Neon), com TODAS as features já mapeadas, e com código PRÓPRIO escrito em
Rust/Go**. A motivação é um **moat de código defensável** (hoje raso — qualquer um compõe Postgres + pgvector +
pgvectorscale + uma casca de IA) e a percepção de "produto de engenharia", não "amontoado de scripts".

Diagnóstico honesto que motivou a virada (medido 2026-06-29): nosso código próprio é ~2.100 LoC SQL + ~4.300 LoC
Python (majoritariamente teste) + plpython3u/plpgsql — o hot-path (engine, índice) vem de terceiros compostos
(Postgres C, pgvector C, pgvectorscale Rust). Moat de código fraco.

## Tensão técnica reconhecida (honestidade — Regra 3)

- **AlloyDB Omni NÃO é escrito em Rust/Go** — é o **PostgreSQL (C)** com módulos + a extensão ScaNN. Neon: o
  compute é Postgres (C); só o storage desagregado é Rust. **Nenhum concorrente sério reescreveu o engine
  Postgres** — são milhões de linhas de C, anos de maturidade, e o wire-protocol é gate de produto.
- Portanto "banco em Rust/Go baseado em Postgres" tem **uma** leitura sã, **confirmada pelo CTO**: o **engine
  permanece o PostgreSQL (C, não-reescrito — ADR 0001 núcleo preservado, A3 'engine do zero' segue rejeitado)**;
  o **código PRÓPRIO** é que passa a ser Rust/Go.

## Decisão

**TheoDB passa a ser um banco Postgres-based com código próprio em Rust/Go**, assim definido (3 decisões de
escopo travadas pelo CTO em 2026-06-29):

1. **Engine = PostgreSQL 17 (C), mantido e não-reescrito.** Wire-compatibility preservada (ADR 0001 núcleo).
   Engine novo do zero permanece **fora de escopo** (ADR 0001 A3).
2. **Código próprio em duas frentes:**
   - **Rust (pgrx)** — camadas *in-engine* (hot-path): nosso índice/quantização (quando justificado por
     benchmark), tipos, e a **reescrita da superfície `ai.*` / NL→SQL / híbrida / unificação / import de
     plpython3u → Rust** como extensão(ões) compilada(s).
   - **Go** — camada de *produto/operação*: operador Kubernetes, control plane, CLI, gateway.
3. **Reescrita incremental com paridade**, NÃO big-bang: feature por feature reescrita em Rust, usando os
   **testes atuais como prova de paridade**; o produto continua funcional a cada passo. ROADMAP-v2 sequencia
   isso.
4. **Todas as features mapeadas** (`docs/features/` 01–12 + o que foi entregue em M0–M16) são preservadas —
   reescritas, não removidas.

## O que cada ADR supersedido vira

| ADR | Antes | Depois (por esta ADR) |
|---|---|---|
| 0001 no-engine-fork | extensão only; engine intocado | **núcleo mantido** (engine Postgres não-reescrito, wire-compat); ampliado: agora construímos **extensões próprias em Rust (pgrx)**, o que o próprio 0001 já permitia (modelo de extensão) |
| 0002 north-star (composição, measurement-first) | compor > construir | **construir** código próprio passa a ser objetivo; **measurement-first permanece** (não forkar/otimizar sem benchmark) |
| 0004 NO-FORK ScaNN | não escrever índice próprio | **reaberto**: índice/quantização próprios em Rust permitidos, **ainda gateados por benchmark** (D3 / measurement-first preservado) |
| 0005 unificação = moat, código mínimo | moat = produto/DX | **ampliado**: o moat agora **inclui código próprio defensável (Rust/Go)**; a unificação continua sendo um pilar do produto |

## O que NÃO muda (invariantes preservados)

- **Wire-compatibility com PostgreSQL** (ADR 0001 núcleo) — gate de produto.
- **Licença permissiva D1** (Apache-2.0; AGPL barrada) — Rust/Go próprio é nosso, permissivo.
- **Measurement-first** (ADR 0002) — índice/quantização própria só com benchmark de gatilho; nada de claim de
  performance sem evidência (`public-copy.md`, Regra 5).
- **Honestidade** (Regra 3) — a reescrita prova paridade pelos testes antes de substituir.

## Consequências

- **Positivas:** código próprio compilado e defensável (Rust in-engine + Go control plane); produto que é um
  banco de marca própria, não uma composição rasa; caminho para o managed/BaaS (control plane Go).
- **Custo/risco:** refundação de **meses**; reescrever camada plpython3u funcional (risco de sunk-cost reverso)
  — mitigado por reescrita **incremental com paridade testada** (o produto nunca quebra). Curva de Rust/pgrx.
- **Manutenção:** assumimos manutenção de código próprio (antes terceirizada às extensões da comunidade) —
  consciente, é o preço do moat.

## Alternativas consideradas

- **Manter a tese de composição (ADR 0005 as-is).** Rejeitada pelo CTO: moat de código raso não satisfaz o
  mandato dos investidores.
- **Engine novo do zero em Rust/Go.** Rejeitada (ADR 0001 A3): multi-anos, perde maturidade/wire-compat do
  Postgres; nenhum concorrente sério faz isso.
- **Big-bang rewrite.** Rejeitada: descarta o funcional, meses sem produto; a incremental-com-paridade entrega
  valor contínuo.
- **Go para extensões in-engine.** Rejeitada: extensões PG de hot-path não se escrevem em Go (pgrx é Rust; C é
  a alternativa) — Go fica no control plane, seu lugar idiomático.

## Quando esta ADR pode mudar

Sign-off do CTO + nota de supersede + CHANGELOG (mesmo lock dos demais).

## Próximos passos

1. Anotar os ADRs 0002/0004/0005 com "Superseded in part by ADR 0006".
2. `ROADMAP-v2`: sequenciar a reescrita incremental (scaffold da extensão Rust/pgrx → reescrita feature-a-feature
   com paridade → control plane Go) — cada milestone via o ciclo completo (discover→plan→implement→review).
