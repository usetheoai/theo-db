---
type: Decision
title: ADR 0006 — Virada estratégica: banco Postgres-based com código próprio em Rust/Go
description: O GOTO deixa de ser composição de peças OSS e passa a ser código próprio compilado — Rust in-engine via pgrx, Go no control plane —, mantendo o engine PostgreSQL intocado.
resource: git:f7c7b93:docs/adr/0006-own-code-postgres-based-rust-go.md
tags: [adr, estrategia, locked, rust, go, pgrx, moat, virada]
adr_id: "0006"
adr_status: Accepted (LOCKED — virada de mandato)
decision_date: 2026-06-29
owner: human:paulohenriquevn
supersedes_in_part: ["0002", "0004", "0005"]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0006
    resource: git:f7c7b93:docs/adr/0006-own-code-postgres-based-rust-go.md
    title: ADR 0006 — Virada estratégica
    last_modified: 2026-06-29
---

A virada de mandato do projeto, e o ADR que reorganizou todos os anteriores. É a fonte de
verdade da estratégia **atual** de produto.

# Contexto

Mudança de mandato do CTO sob pressão de investidores: o GOTO deixa de ser "distribuição que
compõe peças OSS com uma camada fina" e passa a ser **um banco de dados competitivo, de marca
própria, baseado no engine PostgreSQL (modelo AlloyDB/Neon), com código PRÓPRIO em Rust/Go**.

A motivação é um **moat de código defensável**. O diagnóstico medido que a justificou: o código
próprio era ~2.100 LoC de SQL e ~4.300 LoC de Python (majoritariamente teste), com o hot-path —
engine, índice — vindo de terceiros compostos. Moat de código fraco.

# Tensão técnica reconhecida

Registrada com honestidade, porque contradiz uma leitura ingênua do mandato:

- **O AlloyDB Omni não é escrito em Rust/Go** — é o PostgreSQL em C com módulos mais a extensão
  ScaNN. No Neon, o compute é Postgres em C; só o storage desagregado é Rust. **Nenhum
  concorrente sério reescreveu o engine PostgreSQL** — são milhões de linhas de C, anos de
  maturidade, e o wire-protocol é gate de produto.
- Portanto "banco em Rust/Go baseado em Postgres" tem **uma** leitura sã, confirmada pelo CTO:
  o **engine permanece o PostgreSQL, em C, não-reescrito**; o **código próprio** é que passa a
  ser Rust/Go.

# Decisão

Três decisões de escopo travadas:

1. **Engine = PostgreSQL 17 (C), mantido e não-reescrito.** Wire-compatibility preservada — o
   núcleo do [ADR 0001](/decisions/0001-no-engine-fork.md). Engine novo do zero segue fora de
   escopo.
2. **Código próprio em duas frentes:**
   - **Rust via [pgrx](/technologies/pgrx.md)** — camadas *in-engine* (hot-path): índice e
     quantização próprios quando justificados por benchmark, tipos próprios, e a reescrita da
     superfície `ai.*`, NL→SQL, híbrida e de unificação, saindo de plpython3u para extensão
     compilada.
   - **[Go](/technologies/go.md)** — camada de produto e operação: operador Kubernetes,
     control plane, CLI, gateway.
3. **Reescrita incremental com paridade**, não big-bang: feature a feature, usando os testes
   existentes como prova de paridade, com o produto funcional a cada passo.

Todas as features mapeadas são preservadas — **reescritas, não removidas**.

# O que cada ADR supersedido vira

| ADR | Antes | Depois |
|---|---|---|
| [0001](/decisions/0001-no-engine-fork.md) | extensão only; engine intocado | **núcleo mantido**; ampliado: agora construímos extensões próprias em Rust, o que o modelo de extensão já permitia |
| [0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md) | compor > construir | **construir** passa a ser objetivo; **measurement-first permanece** |
| [0004](/decisions/0004-scann-fork-decision.md) | não escrever índice próprio | **reaberto**: índice/quantização próprios permitidos, ainda gateados por benchmark |
| [0005](/decisions/0005-unification-as-differentiator.md) | moat = produto/DX | **ampliado**: o moat inclui código próprio defensável; a unificação segue um pilar |

# Invariantes preservados

Wire-compatibility com o PostgreSQL. Licença permissiva Apache-2.0 com AGPL barrada — código
Rust/Go próprio é nosso e é permissivo. Measurement-first — índice e quantização próprios só
com benchmark de gatilho. E honestidade: a reescrita prova paridade pelos testes antes de
substituir.[^adr0006]

# Consequências

**Positivas:** código próprio compilado e defensável; produto que é um banco de marca própria,
não uma composição rasa; caminho aberto para o managed/BaaS via control plane em Go.

**Custo e risco:** refundação de meses, e reescrever uma camada plpython3u funcional carrega
risco de sunk-cost reverso — mitigado pela reescrita incremental com paridade testada. Curva de
aprendizado de Rust e pgrx. E assume-se manutenção de código que antes era terceirizada às
extensões da comunidade: consciente, é o preço do moat.

# Alternativas rejeitadas

Manter a tese de composição — rejeitada pelo CTO, moat raso não satisfaz o mandato. Engine novo
do zero em Rust/Go — multi-anos, perde maturidade e wire-compat, e nenhum concorrente sério faz
isso. Big-bang rewrite — descarta o funcional e passa meses sem produto. Go para extensões
in-engine — extensões PG de hot-path não se escrevem em Go; o pgrx é Rust, e Go fica no control
plane, seu lugar idiomático.

[^adr0006]: ADR 0006 — Virada estratégica: banco Postgres-based com código próprio em Rust/Go
