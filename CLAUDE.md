# CLAUDE.md — TheoDB

Instruções específicas deste projeto para o Claude Code. Complementam (não substituem)
as regras globais em `~/.claude/CLAUDE.md` e as convenções de ciclo em
`../.claude/rules/`. Quando houver conflito, a regra mais específica vence.

## Contexto

TheoDB é um banco de dados **open-source, PostgreSQL-compatível**, concorrente direto do
**AlloyDB** (nosso alvo SOTA), entregue como **edição downloadable** que roda em qualquer lugar.
Produto definido em [`PRD.md`](./PRD.md); visão e roadmap em [`README.md`](./README.md).
Decisões fechadas: PRD §15 (D1–D7).

---

## Princípio guia do projeto: Esforço ≠ Complexidade

> **Não medimos esforço. Medimos complexidade — e ela é medida pela necessidade do
> projeto, nunca pelo trabalho empregado.**

A complexidade de um sistema deve refletir a dificuldade **essencial** do problema que ele
resolve — não a quantidade de esforço que gastamos, não quão elaborado o código parece, não
quanto tempo investimos. São dois eixos independentes:

- **Esforço ALTO é bem-vindo** quando a necessidade do projeto justifica. Forkar uma extensão,
  escrever uma suíte de benchmark reproduzível, manter um CI de rebase contínuo, perseguir um
  ganho de recall — se há necessidade real e evidência, **não medimos esforço**.
- **Complexidade desnecessária é proibida** — sempre. Abstração prematura, generalização
  especulativa, camada de indireção sem valor, reimplementar o que já existe. Isso é
  complexidade *acidental* (auto-imposta), e nenhum montante de esforço a justifica.

### Complexidade essencial vs. acidental

| | Essencial (aceita) | Acidental (eliminada) |
|---|---|---|
| Origem | O problema/projeto exige | Nós nos impomos |
| Exemplo TheoDB ✅ | Integrar ScaNN-quality ANN ao planner do Postgres | Criar uma abstração "multi-engine" tendo só Postgres |
| Exemplo TheoDB ✅ | Manter fork de `pgvector` com CI de rebase (com benchmark provando o ganho) | Reescrever pgvector do zero "para ter controle" |
| Teste | Remover isso quebra um requisito real? | Remover isso só remove indireção? |

### Regras operacionais derivadas

1. **A decisão de FAZER vem da necessidade do projeto** — não de quão fácil ou difícil é.
   Nunca rejeite a coisa certa por dar trabalho; nunca faça a coisa errada por ser fácil.
2. **A decisão de COMO fazer é a mais simples que resolve** — independentemente do esforço.
   Caminhe a parsimony-ladder (`../.claude/rules/parsimony-ladder.md`) antes de escrever código.
3. **Esforço investido NUNCA justifica manter complexidade (anti-sunk-cost).** "Já gastei
   tempo nisso" não é razão para manter. Se o upstream alcançar nosso fork, desfazemos o fork (D3).
4. **Esforço NUNCA é métrica de valor.** PRs/commits não valem pelo tamanho. Diff menor que
   resolve o mesmo problema é melhor.
5. **Complexidade essencial nunca é cortada em nome de "simplicidade".** Testes, validação de
   borda, error handling, segurança e acessibilidade são essenciais — ver
   `../.claude/rules/parsimony-ladder.md § Never on the chopping block`.

---

## North Star — igualar ou superar o AlloyDB (Opção α)

> **Mandato do CTO (2026-06-27):** entregar um banco **igual ou superior ao AlloyDB**. Fonte de verdade
> LOCKED: [`docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`](./docs/adr/0002-north-star-equal-or-superior-to-alloydb.md).

**Como (Opção α):** igualar/superar o AlloyDB em **capacidades e resultados** para usuários
OSS/on-prem/model-agnostic; **vencer já hoje** em abertura, custo, portabilidade e independência de modelo;
buscar **superioridade de performance no pilar vetorial comprovada por benchmark** (`docs/benchmarks/`).

- **Measurement-first:** o harness de recall@k reproduzível **existe** (`benchmarks/theodb_bench/`) e é
  pré-requisito de qualquer claim de performance. Estado medido: SIFT1M vs ScaNN (M33 — gap ~25× QPS) e vs
  pgvector hnsw (M45 — **paridade** recall×QPS). Nenhuma afirmação de performance sem artefato em `docs/benchmarks/`.
- **VEREDITO MEDIDO FINAL do pilar vetorial (M73, 2026-07-10, `docs/adr/0035` + `docs/benchmarks/m73-headtohead-verdict.md`):**
  **paridade own-code de recall classe-pgvector ALCANÇADA** (M60/M69/M70); **throughput multi-cliente
  competitivo-a-superior** vs pgvector no regime 128d clusterizado (M72, +11% QPS a recall casado); **superioridade
  de QPS vetorial sobre o ScaNN/AlloyDB MEDIDA como NÃO-ALCANÇÁVEL** por extensão PG permissiva (gap ~25-44× @ 0.99 é
  de paradigma — AH-LUT anisotrópico + não pagar o imposto MVCC/WAL; RaBitQ, melhor quantizador permissivo, dá
  memória não QPS — M74/ADR-0036). Posicionamento permitido: "paridade recall + memória billion-scale + AI-native/
  HTAP/aberto"; **jamais** "mais rápido que o AlloyDB no vetor". Reposicionamento formal do North Star: `docs/adr/0033`
  (proposto, decisão do owner — o mandato LOCKED ADR-0002 permanece até assinatura).
- **Fork é condicional** ao benchmark de gatilho (D3); não forkar antes de medir (anti-sunk-cost).
- **Columnar / lakehouse (D2)** é uma aposta **diferente e competitiva**, não cópia do AlloyDB — forçado pela
  licença permissiva (D1 barra AGPL). Desde o **M143 (v0.131.0) é 100% own-code** (colunar in-DB `theodb_columnar`
  + lakehouse Parquet via DataFusion/Arrow) — o **`pg_duckdb` foi removido por completo**, o último componente
  C++/httpfs do projeto (ADR-0056/0057). Paridade interna *literal* (Opção β) exigiria reabrir D1/D2/D7 — fora de
  escopo até novo ADR. **HA / replicação / control-plane são deploy/plataforma — fora do escopo deste
  repositório** (o `operator/` Go e o `ha/` Patroni foram removidos; este repo é o banco: engine + extensão).
- **Esforço ≠ Complexidade:** esforço alto é bem-vindo (ScaNN-as-PG-AM, fork com CI de rebase, suíte de
  benchmark); o COMO é medir-depois-construir-o-essencial. Performance só vira claim com benchmark
  (`../.claude/rules/public-copy.md`).

---

## Regras específicas do TheoDB

1. **Ancore no SOTA AlloyDB.** Decisões de produto/arquitetura espelham o alvo (AlloyDB) e
   se afastam dele apenas com justificativa explícita (ex.: licença, OSS). Ver dossiê em `PRD.md`.
2. **Licença Apache 2.0; AGPL é proibida na distribuição (D1).** Nenhuma dependência AGPL
   entra no pacote. Só Apache 2.0 / MIT / BSD / PostgreSQL License. Due-diligence é gate de
   release (PRD §11; rodar `loop-check-licence`).
3. **Sem fork do engine PostgreSQL.** A regra "sem fork" vale para o engine. **Extensões**
   (`pgvector`/`pgvectorscale`) podem ser forkadas sob a **Política de Fork** (D3): upstream-first,
   gatilho por benchmark reproduzível, diff mínimo, CI de rebase, saída quando o upstream alcançar.
4. **Não reinvente.** Compomos sobre PostgreSQL + extensões maduras (Regra 9). Código próprio
   só onde nenhuma peça OSS permissiva resolve.
5. **Performance é claim, não opinião.** Nenhuma afirmação de performance ("Nx mais rápido")
   sem benchmark reproduzível e publicado em `docs/benchmarks/` (`../.claude/rules/public-copy.md`).
   Metas de design são marcadas como metas, não como fatos.
6. **100% wire-compatible com PostgreSQL é gate**, não feature opcional.
7. **Honestidade extrema (Regra 3).** Diga quando algo é incerto, quando um trade-off existe
   (ex.: nosso columnar/lakehouse é own-code disk/Parquet — DataFusion/Arrow, sem DuckDB desde o M143 —, não
   in-memory-auto como o AlloyDB — D2), e quando uma técnica ainda não foi validada.
8. **Fundamente em referência primária — e o acervo local vem PRIMEIRO.** Nenhuma decisão técnica,
   design ou análise se apoia em memória do modelo quando existe fonte no disco. Antes de opinar
   sobre ANN, colunar, WAL/MVCC, BM25, quantização, grafo ou metodologia de benchmark: **leia o
   paper/código correspondente no acervo** (§ abaixo) e **cite `arquivo:linha` ou o PDF**. Ordem
   obrigatória: **acervo local → web (R0) → conhecimento interno (último recurso, declarado como tal)**.

---

## Acervo de referências (fonte primária — consultar SEMPRE antes de responder)

Catálogo versionado: [`.claude/knowledge-base/references-catalog.md`](./.claude/knowledge-base/references-catalog.md)
— é o contrato; toda referência nova entra lá. Os arquivos são gitignored (2,3 GB) e reprodutíveis
pelos comandos de clone do catálogo.

| Onde | O que tem | Use para |
|---|---|---|
| `.claude/knowledge-base/references/papers/` | **25 PDFs** — HNSW, ScaNN/AQ, RaBitQ, DiskANN, Faiss, IIR (livro), RRF, BEIR, C-Store, MonetDB/X100, Morsel, ARIES, SSI/MVCC, rigor estatístico, GraphRAG, prompt injection | teoria, matemática, e o SOTA contra o qual medimos |
| `.claude/knowledge-base/references/{pgrx,tantivy,faiss,hnswlib,datafusion,arrow-rs,postgres,…}` | **33 repos** de peers, `--depth 1`, grep-áveis | como o SOTA implementa de verdade |
| `.claude/knowledge-base/references/FlameGraph/` | ferramental de profiling | **M148** (spike ativo) e o trabalho de perf do colunar |

**Atalhos por assunto** (o mínimo a abrir antes de tocar no tema):

- **`unsafe` / FFI / pgrx** → `references/pgrx/` (há **384 `unsafe`** no `theodb_rs`; é a classe de
  defeito mais cara já encontrada em review — panic atravessando C, `TopMemoryContext`, MVCC do SPI).
- **Colunar / M148–M151** → `papers/morsel-parallelism-leis-2014.pdf`, `papers/monetdb-x100-boncz-2005.pdf`,
  `references/parquet-format/`, `references/datafusion/` + o survey de Abadi (403 no download — ler online).
- **Vetorial** → `papers/hnsw-*.pdf`, `papers/scann-*.pdf` (**âncora do North Star**, regra 1), `references/hnswlib/`.
- **Lexical / BM25** → `papers/iir-manning-2008-BOOK.pdf` (teoria) + `references/tantivy/` (segmentos, `Directory`).
- **Qualquer afirmação de performance** → `papers/rigorous-perf-eval-georges-2007.pdf` antes de medir
  (regra 5; lições M123/M130/M131: CV ≠ significância pareada, viés de amostragem, ablação mesmo-índice).

**Invioláveis do acervo:** é **read-only** (hook `boundary-check.sh` bloqueia escrita — achados vão para
`knowledge-base/discoveries/blueprints/`); citação que não resolve no disco **não entra** (Regra 3);
peers com licença copyleft (`paradedb`, `citus`, `hydra`, `vectorchord` = AGPL; `FlameGraph` = CDDL) são
**study-only** — copiar código deles para a distribuição é proibido por D1.

---

## Base de conhecimento OKF (consultar ANTES, escrever DEPOIS — inquebrável)

Contrato: [`.claude/rules/okf-knowledge-base.md`](./.claude/rules/okf-knowledge-base.md).
Bundle: `.claude/knowledge-base/okf/` — **46 conceitos** em [Open Knowledge Format v0.1](https://github.com/google/open-knowledge-format),
um arquivo por conceito, links markdown formando um grafo, `type` como único campo obrigatório.

**Por que ele existe.** O conhecimento que este projeto pagou para aprender já estava escrito — em 67 arquivos de
memória, 110 blueprints, notas e mensagens de commit. Espalhado, ele **não morde no momento em que seria útil**.
Numa única sessão do M169, seis diagnósticos caíram por medição e **nenhum era novo em espécie**: todos tinham
precedente registrado em algum lugar que não disparou.

| `type` | A pergunta que ele responde | Quando ler |
|---|---|---|
| `Failure Mode` | "estou prestes a cometer isto?" | **antes de montar qualquer medição** |
| `Technique` | "qual é o método certo aqui?" | idem, e antes de abrir issue de produto |
| `Invariant` | "a plataforma permite isso?" | antes de mexer em storage, FFI, `unsafe`, recovery, build, branch compartilhado |
| `Measurement` | "isso já foi medido?" | **antes de publicar qualquer número** — ele pode já existir |
| `Honest Negative` | "isso já foi tentado e refutado?" | **antes de propor aposta técnica ou milestone** |

**Escrita obrigatória.** Vira conceito: todo número publicado em `docs/benchmarks/**`; toda alegação minha
derrubada por medição; toda aposta medida e refutada; toda propriedade de plataforma aprendida por falha; todo
método que passou a ser exigido. **Atualize o conceito existente** — nunca crie um segundo arquivo para a mesma
classe. Registre a mudança em `okf/log.md`.

**NÃO vira conceito** (e escrever enchimento para satisfazer o gate é pior que não escrever): rastro de execução
— isso é `knowledge-base/` do ciclo; decisão de arquitetura — isso é ADR em `docs/adr/`; bug corrigido sem lição
generalizável.

**Verificação.** `python3 .claude/scripts/check_okf.py` valida quatro invariantes estruturais (`type` presente,
links internos resolvem, índices sincronizados, arquivos-raiz existem) — exit `0`/`1`/`2`. O
`hooks/stop-validation.sh` **BLOQUEIA** em dois casos determinísticos: bundle inválido, e número publicado em
`docs/benchmarks/**` sem conceito `Measurement` tocado. O `hooks/userpromptsubmit-inject.sh` injeta o ponteiro a
cada turno.

**Limite honesto:** nenhum hook prova que eu li. A injeção é o mecanismo mais forte disponível; os gatilhos de
leitura são instruction-grade, como os degraus 2, 3 e 5 da parsimony ladder. Fingir que isso é mecanizável seria
o mesmo `cobertura-alegada-sem-execucao` que o bundle documenta.

## Fluxo de trabalho

- Branch de trabalho: **`develop`** (nunca `main` — Regra 4). `main` recebe só releases.
- Toda mudança visível entra no [`CHANGELOG.md`](./CHANGELOG.md) `[Unreleased]` (Regra 6).
- Commits **sem** trailer `Co-Authored-By` (política do projeto — `../.claude/rules/cycle-review.md`).
- Trabalho não-trivial passa pelos ciclos: `cycle-discover` → `cycle-plan` → `cycle-implement`
  → `cycle-code-quality` → `cycle-review` (ver `../.claude/rules/`).
- Decisões arquiteturais viram ADRs em `docs/adr/` (a estrutura existe — 12 ADRs, incluindo D1–D7
  formalizados em `docs/adr/0006`); toda nova decisão de arquitetura abre um ADR.
