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

- **Base:** PostgreSQL upstream, **sem fork** (recebe patches do upstream; divergência mínima e isolada em extensões). A regra "sem fork" vale para o **engine** — não para extensões.
- **`pgvector*` customizado:** ponto de diferenciação técnica. Fork de `pgvector`/`pgvectorscale` é **autorizado quando trouxer avanço mensurável** (mesma estratégia do AlloyDB, cujo vector é um pgvector customizado), sob a **Política de Fork** (ver D3). Integração com o planner para filtered vector search.
- **Camada columnar:** extensão de armazenamento colunar (DuckDB-powered: `pg_mooncake`/`pg_analytics`) para acelerar analytics — ver due-diligence de licença em [§11](#11-modelo-open-source-e-licenciamento) e D2.
- **HA/replicação:** composição de ferramentas OSS consagradas (ex.: Patroni, pgBackRest) sob um operador único.
- **Composition root única:** o empacotamento e o tuning conjunto são o produto; o usuário não monta peças.

> **Decisão de design (ADR a registrar):** "sem fork do Postgres" é regra. Patches no engine só via extensão ou via contribuição upstream. Isso protege a compatibilidade e a manutenibilidade (evita o destino caro do "Fork do PostgreSQL").

---

## 7. Pilares de capacidade (o produto inteiro)

Cada pilar é uma área de produto independente. O MVP recortará um subconjunto ([§14](#14-recorte-de-mvp-candidato)).

### P1 — Compatibilidade PostgreSQL + empacotamento
Distribuição única, 100% wire-compatible, com extensões pré-instaladas e tunadas. Versões alinhadas a majors suportados do Postgres. Drivers/ferramentas padrão funcionam sem mudança.

### P2 — Vetorial / IA in-database (pilar killer)
- pgvector customizado (`pgvector` + `pgvectorscale`): armazenamento de embeddings + índice ANN de alta performance (StreamingDiskANN) + HNSW/IVFFlat padrão. Ver D3.
- Geração de embeddings via SQL (modelos locais e remotos).
- Filtered vector search com integração ao planner.
- Hybrid search (texto + semântico) e reranking.
- Interface de linguagem natural (NL → SQL) com guarda contra prompt-injection (views parametrizadas seguras).
- Avaliação de recall e tuning de índice.

### P3 — Columnar / HTAP analytics
Armazenamento colunar acelerado por DuckDB (`pg_mooncake` MIT, primário; `pg_analytics` alternativa) para consultas analíticas rápidas sobre dados Postgres, sem ETL para outro sistema. Entrega o resultado HTAP do AlloyDB com peça permissiva (ver D2 — é columnar lakehouse, não in-memory).

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
- [SHOULD] Armazenamento colunar (DuckDB-powered) para colunas/tabelas selecionadas.
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

**Licença (DECIDIDA — ver D1):** **Apache License 2.0**, a mesma do Supabase. Arquivo `LICENSE` na raiz do repositório.
**Regra dura derivada:** Apache 2.0 é permissiva → **dependências AGPL são proibidas** na distribuição. Só Apache 2.0 / MIT / BSD / PostgreSQL License entram no pacote.

**Due-diligence de licença das dependências (verificada em 2026-06-26 — BLOQUEANTE antes de empacotar):**

| Dependência candidata | Licença | Veredito |
|---|---|---|
| PostgreSQL (base) | PostgreSQL License | ✅ permitida |
| `pgvector` (vetorial — HNSW/IVFFlat) | PostgreSQL License | ✅ permitida |
| `pgvectorscale` (ANN — StreamingDiskANN) | PostgreSQL License | ✅ permitida |
| `pg_mooncake` (columnar — primário) | MIT | ✅ permitida |
| `pg_analytics` / ParadeDB (columnar — alternativa) | PostgreSQL License | ✅ permitida |
| Citus columnar | **AGPL-3.0** | ❌ **barrada** |
| Hydra columnar engine | **AGPL-3.0** | ❌ **barrada** |
| ParadeDB `pg_search` (full-text BM25) | **AGPL-3.0** | ❌ **barrada** (buscar alternativa permissiva p/ full-text) |
| Patroni / pgBackRest (HA/backup) | a confirmar no discovery | ⏳ validar |

> O plugin `loop-check-licence` **deve** rodar sobre o conjunto de dependências antes de qualquer release. Qualquer dependência AGPL (ou sem licença) é um **gate de release** — bloqueia o pacote até substituição por peça permissiva.

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
| **Manutenção do fork de pgvector/pgvectorscale** diverge do upstream | Média | Política de Fork (D3): upstream-first, gatilho por benchmark, diff mínimo, CI de rebase contínuo, desfazer o fork quando o upstream alcançar. |
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

## 15. Decisões (fechadas em 2026-06-26)

As 7 questões em aberto foram resolvidas. Critério de decisão: **espelhar o SOTA do AlloyDB** (nosso alvo) usando **apenas peças OSS permissivas** — porque a licença escolhida (Apache 2.0) bane copyleft de rede (AGPL). Cada decisão tem consequência rastreável.

### D1 — Licença: **Apache 2.0** (a mesma do Supabase)
O código próprio do TheoDB é licenciado sob **Apache License 2.0**, igual ao Supabase. Inclui grant de patente, é amigável a adoção corporativa e compatível com as dependências permissivas escolhidas.
**Consequência (regra dura):** toda dependência empacotada DEVE ser permissiva (Apache 2.0 / MIT / BSD / PostgreSQL License). **AGPL é proibida** na distribuição. Sem open-core: os pilares centrais ficam todos sob Apache 2.0 (Princípio 3).

### D2 — Camada columnar: **DuckDB-powered permissivo** (`pg_mooncake` MIT, primário; `pg_analytics` PostgreSQL License, alternativa)
O columnar in-memory do AlloyDB é proprietário e não tem equivalente OSS permissivo idêntico. As opções columnar mais óbvias do ecossistema (**Citus columnar, Hydra columnar engine, ParadeDB pg_search**) são **AGPL-3.0 → barradas por D1**. Adotamos **`pg_mooncake` (MIT)** como candidato primário e **`pg_analytics` (PostgreSQL License)** como alternativa — ambos columnar/lakehouse acelerados por DuckDB.
**Honestidade:** é columnar on-disk/lakehouse (DuckDB+Iceberg), *não* o columnar in-memory do AlloyDB. Entregamos o **resultado** (HTAP/analytics rápido sobre dados Postgres) com técnica diferente. A escolha final entre as duas peças é validada no `cycle-discover` do pilar P3.

### D3 — Índice ANN: **pgvector + pgvectorscale**, com fork autorizado (Política de Fork)
O AlloyDB usa ScaNN (a integração `alloydb_scann` é proprietária). Nosso "pgvector customizado" = **`pgvector`** (PostgreSQL License — HNSW/IVFFlat) + **`pgvectorscale`** (PostgreSQL License — índice **StreamingDiskANN** + Statistical Binary Quantization). Espelha o "vector + índice ANN avançado" do AlloyDB com peças compatíveis com D1, e dá o caminho de escala disco/bilhões de vetores (DiskANN) sem depender de código fechado.

**Política de Fork (decisão do owner, 2026-06-26 — "não vamos medir esforços onde houver avanço"):** forkar `pgvector`/`pgvectorscale` é **autorizado** quando trouxer vantagem competitiva real. A licença permite (PostgreSQL License → redistribuível sob Apache 2.0 com atribuição) e o próprio AlloyDB faz isso. Para o fork ser um ativo, não um fardo, ele segue um contrato:

1. **Upstream-first.** A mudança é proposta ao upstream antes de virar fork. Fork é o que sobra quando o upstream não absorve (ou não cabe no nosso roadmap de tempo).
2. **Gatilho por evidência.** O fork dispara no `cycle-discover`/`cycle-plan` do pilar vetorial, justificado por **benchmark reproduzível** (latência/recall/escala) — nunca às cegas. Sem ganho mensurável, ficamos no upstream (KISS/YAGNI).
3. **Diff mínimo e isolado.** Patches pequenos e bem delimitados; nada de reescrever a extensão.
4. **CI de rebase contínuo.** Pipeline que faz merge das novas versões do upstream e roda a suíte; divergência é dívida visível, não silenciosa.
5. **Escopo:** vale para **extensões** (pgvector/pgvectorscale e afins). **Não** se estende ao engine PostgreSQL, que permanece sem fork (§6).
6. **Saída:** se o upstream alcançar o nosso patch, **desfazemos o fork** e voltamos ao upstream.

Enquanto não houver evidência de ganho, o default permanece: **upstream as-is**.

### D4 — Telemetria: **opt-in, anônima, mínima, desligada por padrão**
Diferente do AlloyDB managed (que coleta por ser serviço), o TheoDB é OSS instalável. Telemetria é **opt-in explícito**, anônima, mínima (versão, OS/arch, contagem de recursos), documentada e desativável a qualquer momento. Confiança > métrica.

### D5 — Majors do PostgreSQL: **17 (MVP) → 18**
AlloyDB cobre PG 14–17 e o Omni já tem builds 18.x. Para começar enxuto (KISS/YAGNI), o MVP mira **PostgreSQL 17** (maduro e amplamente adotado) e adiciona **PostgreSQL 18** em seguida.
**Dependência:** condicionado a `pgvector`/`pgvectorscale`/columnar suportarem o major — a confirmar no discovery antes de travar 18.

### D6 — Governança: **DCO (sign-off), sem CLA**
Modelo open governance com **Developer Certificate of Origin** (sign-off por commit), **sem CLA** (menos atrito, sem transferência de copyright — coerente com Apache 2.0 e com "OSS de verdade"). Mantenedores core da usetheo inicialmente; `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md` e `SECURITY.md` a criar. Sem open-core nos pilares centrais.

### D7 — Control plane managed: **fora do v1; porta aberta via operador K8s**
v1 = engine downloadable OSS (decisão já firmada). Um managed **não** será construído agora (YAGNI), mas a arquitetura mantém a porta aberta: o **operador Kubernetes (P8)** é o substrato natural de um eventual managed. Reavaliar pós-tração.

> Estas decisões devem ser promovidas a ADRs formais (`docs/adr/0001..0007`) quando a estrutura de engenharia for criada. Mudá-las exige novo ADR + entrada no CHANGELOG.

---

## Referências

- Dossiê de pesquisa do AlloyDB (concorrente de referência): coletado em 2026-06-26 das páginas oficiais `cloud.google.com/products/alloydb`, `/alloydb/pricing`, `/alloydb/ai`, `/alloydb/omni`.
- Convenções do repositório: `.claude/rules/` (cycle-discover, cycle-plan, parsimony-ladder, public-copy, testing, architecture).
