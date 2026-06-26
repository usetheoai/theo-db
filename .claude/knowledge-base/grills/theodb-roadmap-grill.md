---
slug: theodb
date: 2026-06-26
generated_by: roadmap-init
questions_answered: 7
unresolved_dims: []
status: completed
source: synthesized from PRD.md (§1–§15) + README.md (roadmap macro) — user confirmed batch (see AskUserQuestion 2026-06-26)
---

# Roadmap grill: theodb

> As 7 dimensões foram sintetizadas das fontes já documentadas (PRD §1–§15, README) e
> confirmadas pelo usuário em lote — o projeto pediu explicitamente "baseado no roadmap
> já documentado". Re-entrevistar uma a uma o que já está decidido em PRD §15 (D1–D7)
> seria cerimônia contra KISS/Regra 1.

### Q1/7: Root problem

**Question:** Qual o problema-raiz que o TheoDB resolve, e para quem dói hoje?

**Recommended:** Times que querem PostgreSQL com IA+analytics escolhem entre opções ruins.

**User answer:** Equipes que querem PostgreSQL com IA e analytics escolhem entre: (a) Postgres
vanilla + montar extensões à mão (integração frágil, sem tuning conjunto); (b) AlloyDB Omni
proprietário ($40/vCPU/mês, código fechado, sem auditoria); (c) AlloyDB managed (lock-in GCP,
não roda on-prem/edge); (d) bancos vetoriais dedicados (mais um sistema, ETL, sem SQL unificado).
Existe um vácuo: ninguém oferece o "AlloyDB Omni open-source". (PRD §2)

### Q2/7: Primary users

**Question:** Quem são os usuários primários?

**Recommended:** Devs de aplicação gen-AI; secundários eng. plataforma/DBA, edge/regulado, anti-lock-in, OSS.

**User answer:** Primário: dev de aplicação gen-AI (RAG/agentes) — quer embeddings + vector search
no mesmo banco dos dados operacionais, via SQL, sem ETL. Secundários: engenheiro de plataforma/DBA
(on-prem/multicloud, sem licença por vCPU), times com requisito de soberania/edge (regulado, air-gap),
empresas fugindo de lock-in/custo, contribuidores OSS. Externos, comunidade aberta. (PRD §4)

### Q3/7: In scope (V1)

**Question:** O que é in-scope para o V1 (MVP)?

**Recommended:** Core+empacotamento + vetorial/IA + imagem container + migração mínima + due-diligence de licença.

**User answer:** MVP candidato "TheoDB Core + Vector" (M0→M2): P1 compatibilidade+empacotamento
(PostgreSQL 17, imagem container) [MUST]; P2 vetorial/IA — pgvector+pgvectorscale com índice ANN
avançado (StreamingDiskANN) + geração de embeddings via SQL [MUST]; P8 imagem container oficial
[MUST]; P9 migração mínima (import/export, vir do Postgres vanilla) [SHOULD]; licença +
due-diligence das deps do MVP [MUST — gate]. (PRD §14)

### Q4/7: Explicitly out of scope

**Question:** O que é explicitamente fora de escopo?

**Recommended:** Columnar/HA completos, operador, NL→SQL, observabilidade, MCP — fases seguintes.

**User answer:** Fora do MVP: columnar/HTAP (P3), HA/DR completo (P4), read pools (P5), operador
K8s/RPM avançado (P8 avançado), NL→SQL (P2 avançado), observabilidade/auto-tuning (P7), MCP server
e integrações (P10). Fora do produto: engine novo / fork divergente do PostgreSQL; serviço gerenciado
de nuvem na v1 (porta aberta via operador K8s, fora do escopo inicial — D7); paridade de performance
com o engine proprietário do Google. (PRD §3, §14, D7)

### Q5/7: Hard constraints

**Question:** Quais as constraints duras (stack, licença, compliance, runtime)?

**Recommended:** Apache 2.0 / AGPL proibida; sem fork do engine; wire-compat gate; PG17→18.

**User answer:** Licença Apache 2.0 (D1) → dependências AGPL PROIBIDAS na distribuição (só Apache 2.0
/ MIT / BSD / PostgreSQL License). Sem fork do engine PostgreSQL (extensões podem ser forkadas sob a
Política de Fork D3: upstream-first, gatilho por benchmark, diff mínimo, CI de rebase, saída quando
upstream alcançar). 100% wire-compatible com PostgreSQL é gate, não feature. Roda em qualquer lugar
(base Linux + SSD). PostgreSQL 17 (MVP) → 18. Telemetria opt-in/anônima/off por padrão (D4).
Governança DCO sem CLA (D6). (PRD §5, §11, §15)

### Q6/7: Measurable V1 ship criterion

**Question:** Qual o critério mensurável de ship do V1?

**Recommended:** 100% testes de regressão upstream + container PG17+pgvector com vector search funcional + licença limpa.

**User answer:** Compatibilidade: 100% dos testes de regressão do PostgreSQL upstream passam na
distribuição. Imagem container oficial roda PG17 com pgvector+pgvectorscale, aceita conexão wire de
qualquer driver Postgres sem mudança, e executa vector search (tipo vetorial + operadores `<=>` +
índice ANN) end-to-end. Due-diligence de licença limpa (zero AGPL no pacote — gate via
loop-check-licence). Performance é meta de design com benchmark reproduzível, nunca claim sem
evidência. (PRD §9, §11, public-copy)

### Q7/7: North-star metric

**Question:** Qual a métrica north-star?

**Recommended:** Casos de migração saindo de AlloyDB pago → TheoDB.

**User answer:** Casos de migração saindo de AlloyDB pago → TheoDB — a prova de que o "AlloyDB OSS"
tem tração real e ocupa o vácuo de mercado. Métricas de apoio: pulls/downloads da imagem, contribuidores
externos, % de testes de compat Postgres passando, cobertura de benchmark publicado. (PRD §12)
