# PRD — TheoDB

> Product Requirements Document — documento técnico-direto (interno).
> **Status:** Draft v0.1 · **Data:** 2026-06-26 · **Owner:** usetheo
> **Escopo deste PRD:** define o **produto inteiro** (visão de longo prazo). O recorte de **MVP** está na seção [§14](#14-recorte-de-mvp-candidato), a ser fechado num passo seguinte.

---

## 1. Resumo executivo

**TheoDB** é um banco de dados **open-source, PostgreSQL-compatível**, empacotado como uma **edição única para download que roda em qualquer lugar** (laptop, on-prem, edge, qualquer nuvem, Kubernetes, bare metal). Ele combina o engine maduro do PostgreSQL com um conjunto curado de extensões — com destaque para uma versão **customizada do pgvector** — para entregar, num só pacote, três capacidades que hoje exigem montagem manual ou produtos proprietários:

1. **Busca vetorial / IA in-database** (embeddings + vector search de alta performance + linguagem natural).
2. **Analytics colunar / HTAP** (consultas analíticas rápidas sobre dados transacionais vivos).
3. **Operação de produção** (alta disponibilidade, backup/PITR, observabilidade, auto-tuning) via operador e orquestradores.

**Tese central:** o AlloyDB Omni provou que existe demanda por "PostgreSQL + IA + analytics empacotados, rodando em qualquer lugar" — mas ele é **proprietário e pago ($40/vCPU/mês)**. O TheoDB ocupa exatamente essa lacuna sendo **100% open-source**, sem licença por vCPU, sem lock-in, com a mesma técnica de base (Postgres + extensões + pgvector customizado).

---

## 2. Problema e oportunidade

### 2.1 O problema do usuário

Equipes que querem PostgreSQL com IA e analytics hoje escolhem entre opções ruins:

| Opção atual | Dor |
|---|---|
| **Postgres vanilla + montar extensões à mão** | Integração frágil; cada extensão (pgvector, columnar, HA) é instalada/operada separadamente; sem tuning conjunto; sem garantia de compatibilidade entre versões. |
| **AlloyDB Omni** | Resolve o empacotamento, mas é **proprietário** e custa **$40/vCPU/mês**; código fechado impede auditoria, customização e contribuição; trial-grátis só para dev/não-comercial. |
| **AlloyDB managed (nuvem)** | Lock-in no Google Cloud; não roda on-prem/edge; preço por consumo. |
| **Bancos vetoriais dedicados** (Pinecone, etc.) | Mais um sistema para operar; ETL para sincronizar com o Postgres transacional; sem SQL unificado. |

### 2.2 A oportunidade

- O ecossistema PostgreSQL de extensões está **maduro o suficiente** para compor a maior parte do valor do AlloyDB sem reinventar o engine (Regra 9 — Não Reinvente a Roda).
- Existe um **vácuo de mercado**: ninguém oferece o "AlloyDB Omni open-source". Postgres distros (Supabase, Tembo, Percona) tocam partes, mas nenhuma entrega o pacote IA+columnar+ops com a integração e o posicionamento do AlloyDB, sob licença aberta.
- IA generativa tornou **vector search in-database** um requisito de primeira classe — é a frente onde o valor percebido é maior e onde OSS pode brilhar.

---

## 3. Posicionamento e diferenciação

**Posicionamento (uma frase):** *TheoDB é o PostgreSQL com superpoderes de IA e analytics — open-source e rodando em qualquer lugar.*

| Eixo | TheoDB | AlloyDB Omni | Postgres + extensões à mão |
|---|---|---|---|
| Licença | **Open-source** | Proprietário | Open-source |
| Custo de licença | **Zero** | $40/vCPU/mês | Zero |
| Roda em qualquer lugar | ✅ | ✅ | ✅ (você monta) |
| IA/vector empacotado e tunado | ✅ | ✅ | ❌ (manual) |
| Columnar/HTAP empacotado | ✅ (meta) | ✅ | ❌ (manual) |
| HA/backup/operador integrados | ✅ (meta) | ✅ | ❌ (manual) |
| Auditável / customizável / contribuível | ✅ | ❌ | ✅ |
| 100% wire-compatible com Postgres | ✅ | ✅ | ✅ |

**O que NÃO somos (escopo negativo, anti-over-promise):**
- Não somos um engine novo nem um fork divergente do PostgreSQL (ver [§6](#6-arquitetura-de-alto-nível)).
- Não somos (na v1) um serviço gerenciado de nuvem — somos a **edição downloadable**. Um control plane managed é possível no futuro, fora do escopo inicial.
- Não prometemos paridade de performance com o engine proprietário do Google. Nossas metas de performance são próprias e mensuráveis ([§9](#9-requisitos-não-funcionais-metas)).

---

## 4. Público-alvo e personas

| Persona | Quem é | O que quer do TheoDB |
|---|---|---|
| **Dev de aplicação gen-AI** | Constrói RAG/agentes | Embeddings + vector search no mesmo banco dos dados operacionais, via SQL, sem ETL. |
| **Engenheiro de plataforma / DBA** | Opera bancos on-prem/multicloud | Um pacote PostgreSQL com HA, backup, operador K8s e tuning — sem licença por vCPU. |
| **Time com requisito de soberania/edge** | Setor regulado, on-prem, air-gap | Banco que roda fora da nuvem, auditável (código aberto), sem telefonar para casa. |
| **Empresa fugindo de lock-in/custo** | Saindo de DB legado ou de AlloyDB pago | Compatibilidade Postgres + recursos avançados sem custo de licença e sem aprisionamento. |
| **Contribuidor OSS** | Comunidade | Código aberto para auditar, estender e contribuir. |

---

## 5. Princípios de produto

1. **Não reinventar o engine.** Compomos sobre PostgreSQL e extensões maduras; só escrevemos código onde nenhuma peça OSS resolve (Regra 9, KISS).
2. **100% wire-compatible.** Qualquer cliente/driver/ferramenta Postgres funciona sem mudança. Compatibilidade é gate, não feature.
3. **Open-source de verdade.** Licença aberta, sem "open-core" que esconde os recursos centrais atrás de pago.
4. **Roda em qualquer lugar.** Mesma imagem do laptop ao bare metal regulado.
5. **Bateria inclusa, mas desmontável.** Pacote integrado e tunado, mas cada componente é padrão e substituível (sem lock-in interno).
6. **Honestidade de marketing.** Sem "production-ready"/"battle-tested"/comparações de performance sem benchmark reproduzível e publicado (regra `public-copy`).

---

## 6. Arquitetura de alto nível

```
┌─────────────────────────────────────────────────────────────┐
│  TheoDB — distribuição única (container / RPM / K8s operator)│
│                                                              │
│   ┌──────────────────────────────────────────────────────┐  │
│   │            PostgreSQL (upstream, sem fork)            │  │
│   └──────────────────────────────────────────────────────┘  │
│   ┌─────────────┬─────────────┬─────────────┬────────────┐  │
│   │ pgvector*   │ columnar    │ HA / repl.  │ AI/ML      │  │
│   │ (customiz.) │ (analytics) │ (operador)  │ integration│  │
│   └─────────────┴─────────────┴─────────────┴────────────┘  │
│   ┌──────────────────────────────────────────────────────┐  │
│   │  Tooling: CLI, operador K8s, MCP server, migração,   │  │
│   │  observabilidade, index advisor, autovacuum adaptativo│  │
│   └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
        roda em: laptop · on-prem · edge · qualquer nuvem · K8s · bare metal
```

- **Base:** PostgreSQL upstream, **sem fork** (recebe patches do upstream; divergência mínima e isolada em extensões).
- **`pgvector*` customizado:** ponto de diferenciação técnica. Fork mantido do pgvector com índice ANN de alta performance (alvo: qualidade tipo ScaNN, derivável de OSS) + integração com o planner para filtered vector search.
- **Camada columnar:** extensão de armazenamento colunar em memória para acelerar analytics (avaliar Hydra/`pg_mooncake`/colunar próprio — ver due-diligence de licença em [§11](#11-modelo-open-source-e-licenciamento)).
- **HA/replicação:** composição de ferramentas OSS consagradas (ex.: Patroni, pgBackRest) sob um operador único.
- **Composition root única:** o empacotamento e o tuning conjunto são o produto; o usuário não monta peças.

> **Decisão de design (ADR a registrar):** "sem fork do Postgres" é regra. Patches no engine só via extensão ou via contribuição upstream. Isso protege a compatibilidade e a manutenibilidade (evita o destino caro do "Fork do PostgreSQL").

---

## 7. Pilares de capacidade (o produto inteiro)

Cada pilar é uma área de produto independente. O MVP recortará um subconjunto ([§14](#14-recorte-de-mvp-candidato)).

### P1 — Compatibilidade PostgreSQL + empacotamento
Distribuição única, 100% wire-compatible, com extensões pré-instaladas e tunadas. Versões alinhadas a majors suportados do Postgres. Drivers/ferramentas padrão funcionam sem mudança.

### P2 — Vetorial / IA in-database (pilar killer)
- pgvector customizado: armazenamento de embeddings + índice ANN de alta performance + HNSW padrão.
- Geração de embeddings via SQL (modelos locais e remotos).
- Filtered vector search com integração ao planner.
- Hybrid search (texto + semântico) e reranking.
- Interface de linguagem natural (NL → SQL) com guarda contra prompt-injection (views parametrizadas seguras).
- Avaliação de recall e tuning de índice.

### P3 — Columnar / HTAP analytics
Engine colunar em memória para acelerar consultas analíticas sobre dados transacionais, sem ETL para outro sistema. Planner escolhe plano row vs colunar.

### P4 — Alta disponibilidade, DR e backup
Failover automático (primary + standby), replicação assíncrona cross-site, backup contínuo + PITR, backups on-demand e agendados.

### P5 — Escala de leitura
Read replicas / read pools com load-balancing; escala horizontal de leitura.

### P6 — Segurança e compliance
Encryption at rest, chaves gerenciadas pelo cliente (CMEK-equivalente), TLS/SSL enforced, integração com gerenciadores de segredo, RBAC, auditoria (pgAudit), políticas de senha, data residency.

### P7 — Observabilidade e auto-tuning
Query insights, index advisor (recomenda índices a partir do workload), autovacuum adaptativo, gestão de memória/storage, recomendações de sizing, métricas/exporters padrão (Prometheus/OpenTelemetry).

### P8 — Deploy anywhere
Imagens container (Docker/Podman, base Debian e UBI), **operador Kubernetes** (GKE/EKS/AKS/OpenShift/CNCF), **orquestrador RPM** (RHEL/Rocky), bare metal e VM. Requisito de base: Linux + SSD.

### P9 — Migração
Migração de Postgres vanilla, de Cloud SQL, **de AlloyDB** (caminho de saída — anti-lock-in), e de bancos vetoriais dedicados. Ferramentas de import/export padrão.

### P10 — Developer experience / tooling
CLI de gestão, **MCP server** para acesso seguro de agentes de IA, integrações com frameworks (LangChain/LlamaIndex), Studio/UI de administração, quickstarts.

---

## 8. Requisitos funcionais (por pilar, alto nível)

> Notação: **[MUST]** essencial ao produto · **[SHOULD]** importante · **[COULD]** desejável.

**P1 Compatibilidade**
- [MUST] Aceitar conexões via protocolo wire do PostgreSQL sem alteração de cliente.
- [MUST] Empacotar extensões pré-instaladas e habilitáveis via `CREATE EXTENSION`.
- [MUST] Suportar ao menos 2 majors recentes do PostgreSQL.

**P2 Vetorial/IA**
- [MUST] Tipo de coluna vetorial + operadores de similaridade (`<=>` etc.).
- [MUST] Índice ANN de alta performance além do HNSW.
- [MUST] Função SQL para gerar embeddings a partir de modelo configurável.
- [SHOULD] Filtered vector search integrado ao planner.
- [SHOULD] Hybrid search + reranking.
- [SHOULD] NL → SQL com views parametrizadas seguras.
- [COULD] Embeddings multimodais; auto-embedding de tabelas.

**P3 Columnar/HTAP**
- [SHOULD] Armazenamento colunar em memória para colunas/tabelas selecionadas.
- [SHOULD] Escolha automática row vs colunar pelo planner.
- [COULD] Recomendação automática de quais colunas materializar.

**P4 HA/DR**
- [MUST] Primary com standby e failover automático.
- [MUST] Backup contínuo + PITR e backups agendados.
- [SHOULD] Replicação cross-site assíncrona.

**P5 Escala de leitura**
- [SHOULD] Read replicas com load-balancing.

**P6 Segurança**
- [MUST] Encryption at rest + TLS enforced.
- [MUST] RBAC e auditoria.
- [SHOULD] Chaves gerenciadas pelo cliente; integração com secret manager.

**P7 Observabilidade**
- [MUST] Métricas exportáveis (Prometheus/OTel).
- [SHOULD] Index advisor + autovacuum adaptativo + query insights.

**P8 Deploy**
- [MUST] Imagem container oficial.
- [MUST] Operador Kubernetes.
- [SHOULD] Orquestrador RPM; bare metal.

**P9 Migração**
- [MUST] Import/export padrão; migração de Postgres vanilla.
- [SHOULD] Migração de AlloyDB e de Cloud SQL.

**P10 DX/tooling**
- [SHOULD] CLI de gestão; MCP server.
- [COULD] UI de administração; integrações LangChain/LlamaIndex.

---

## 9. Requisitos não-funcionais (metas)

> **Honestidade:** estas são **metas de design / SLOs alvo**, não claims. Cada uma exige benchmark reproduzível e publicado (`docs/benchmarks/`) antes de virar afirmação pública.

- **Compatibilidade:** 100% dos testes de regressão do PostgreSQL upstream passam na distribuição.
- **Performance vetorial (meta):** latência e recall competitivos com pgvector/HNSW em datasets de referência (alvo a definir com benchmark; ex.: p95 sob limite X em N vetores com recall ≥ Y%).
- **Overhead de empacotamento (meta):** sobreposição de recursos do TheoDB vs Postgres vanilla mensurável e justificada.
- **Disponibilidade (meta de design):** failover automático sob tempo-alvo a medir; sem SLA numérico publicado até evidência sustentada.
- **Portabilidade:** mesma imagem roda em laptop e bare metal sem rebuild.
- **Footprint:** rodar em ambiente de dev com requisito mínimo documentado (ex.: Linux + SSD + memória/CPU alvo).

---

## 10. Comparativo de mercado

| Capacidade | TheoDB (alvo) | AlloyDB Omni | Postgres vanilla | Supabase/Tembo (distros OSS) |
|---|---|---|---|---|
| Licença aberta | ✅ | ❌ | ✅ | ✅ |
| Custo de licença | Zero | $40/vCPU/mês | Zero | Zero |
| Vector search avançado empacotado | ✅ | ✅ | parcial (pgvector cru) | parcial |
| Columnar/HTAP empacotado | ✅ | ✅ | ❌ | parcial |
| HA/backup/operador integrados | ✅ | ✅ | ❌ | parcial |
| NL → SQL / IA integrada | ✅ | ✅ | ❌ | ❌ |
| Roda em qualquer lugar (1 imagem) | ✅ | ✅ | ✅ | varia |
| Posicionamento "AlloyDB OSS" | ✅ | — | — | não declarado |

---

## 11. Modelo open-source e licenciamento

**Premissa:** open-source de verdade (Princípio 3). Sem open-core que esconda os pilares centrais.

**Licença proposta (questão aberta — decisão de owner + ADR):** Apache 2.0 para o código próprio (permissiva, amigável a adoção corporativa). Alternativa: PostgreSQL License (alinhada ao upstream).

**Due-diligence de licença das dependências (BLOQUEANTE antes de empacotar):**
- PostgreSQL — PostgreSQL License (permissiva) ✅
- pgvector — PostgreSQL License (permissiva) ✅
- Índice ANN tipo ScaNN — base de referência do Google é Apache 2.0; validar a peça exata adotada.
- **Camada columnar — ⚠️ risco de licença:** algumas opções (ex.: Citus columnar) são **AGPL**, o que é incompatível com produto permissivo. Avaliar alternativas Apache/MIT (ex.: `pg_mooncake`, Hydra) com auditoria formal.
- Ferramentas de HA/backup (Patroni, pgBackRest) — validar licença de cada uma.

> Há um plugin de auditoria de licença disponível (`loop-check-licence`) que **deve** rodar sobre o conjunto de dependências antes de qualquer release. Conflito copyleft/permissivo é um gate de release.

---

## 12. Métricas de sucesso

| Categoria | Métrica candidata |
|---|---|
| Adoção | Downloads/pulls da imagem; estrelas/forks; clusters ativos reportados (telemetria opt-in). |
| Comunidade | Contribuidores externos; issues respondidas; cadência de releases. |
| Produto | % de testes de compat. Postgres passando; cobertura de benchmark publicado; nº de extensões integradas e tunadas. |
| Diferenciação | Casos de migração saindo de AlloyDB pago → TheoDB. |

---

## 13. Riscos e mitigações

| Risco | Severidade | Mitigação |
|---|---|---|
| **Columnar é tecnicamente o mais difícil** de igualar | Alta | Tratar como pilar posterior; começar pelo vetorial/IA; usar OSS existente, não reinventar. |
| **Conflito de licença** (AGPL em columnar) | Alta | Due-diligence bloqueante ([§11](#11-modelo-open-source-e-licenciamento)); escolher só deps permissivas. |
| **Manutenção do fork do pgvector** diverge do upstream | Média | Minimizar diff; contribuir upstream; CI contra novas versões do pgvector. |
| **Paridade de performance** com engine proprietário do Google | Média | Não prometer paridade; competir em abertura/custo; metas próprias com benchmark honesto. |
| **Escopo gigante** (10 pilares) | Alta | MVP estreito ([§14](#14-recorte-de-mvp-candidato)); Regra 2 (não abrir pilar novo antes de fechar o anterior). |
| **Posicionamento "AlloyDB killer"** pode soar vendor-hostile | Baixa | Seguir `public-copy`: posicionamento por outcome, não por ataque a concorrente. |

---

## 14. Recorte de MVP (candidato)

> **A FECHAR num passo seguinte.** Proposta inicial de recorte, dada a decisão de "pilar killer = vetorial/IA" e "engine downloadable OSS".

**MVP candidato — "TheoDB Core + Vector":**
- P1 Compatibilidade + empacotamento (1 major do Postgres, imagem container). **[MUST]**
- P2 Vetorial/IA — pgvector customizado com índice ANN avançado + geração de embeddings via SQL. **[MUST]**
- P8 Deploy — imagem container oficial (operador K8s fica para depois). **[MUST]**
- P9 Migração mínima — import/export + vir do Postgres vanilla. **[SHOULD]**
- Licença + due-diligence das deps do MVP. **[MUST — gate]**

**Fora do MVP (fases seguintes):** columnar/HTAP (P3), HA/DR completo (P4), read pools (P5), operador K8s/RPM (P8 avançado), NL→SQL (P2 avançado), observabilidade/auto-tuning (P7), MCP server e integrações (P10).

---

## 15. Questões em aberto

1. **Licença final** do código próprio (Apache 2.0 vs PostgreSQL License)? — decisão de owner + ADR.
2. **Peça columnar** definitiva (Hydra / `pg_mooncake` / outra) e sua licença?
3. **Origem do índice ANN avançado** (qual implementação OSS, e como mantê-la)?
4. **Telemetria opt-in** para métricas de adoção — sim/não e que dados?
5. **Quais majors do PostgreSQL** suportar no v1?
6. **Modelo de governança** do projeto OSS (mantenedores, CLA/DCO)?
7. Haverá, no futuro, um **control plane managed** (fora do escopo v1, mas afeta arquitetura)?

---

## Referências

- Dossiê de pesquisa do AlloyDB (concorrente de referência): coletado em 2026-06-26 das páginas oficiais `cloud.google.com/products/alloydb`, `/alloydb/pricing`, `/alloydb/ai`, `/alloydb/omni`.
- Convenções do repositório: `.claude/rules/` (cycle-discover, cycle-plan, parsimony-ladder, public-copy, testing, architecture).
