# CLAUDE.md — TheoDB

Instruções específicas deste projeto para o Claude Code. Complementam (não substituem)
as regras globais em `~/.claude/CLAUDE.md`. Quando houver conflito, a regra mais
específica vence.

> **Nota de layout (2026-08).** Duas coisas que este arquivo cita não são mais
> navegáveis: `../.claude/rules/` era o workspace pai do layout antigo — hoje este
> repo é um irmão plano em `theo-platform/`, e esse diretório não existe; e o
> `.claude/` local (rules, skills, catálogo do acervo, bundle OKF) saiu do
> versionamento na limpeza de 2026-08. O texto abaixo descreve os contratos como
> foram acordados; os arquivos citados vivem fora do repositório.

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
> LOCKED: [`wiki/decisions/0002-north-star-equal-or-superior-to-alloydb.md`](./wiki/decisions/0002-north-star-equal-or-superior-to-alloydb.md).

**Como (Opção α):** igualar/superar o AlloyDB em **capacidades e resultados** para usuários
OSS/on-prem/model-agnostic; **vencer já hoje** em abertura, custo, portabilidade e independência de modelo;
buscar **superioridade de performance no pilar vetorial comprovada por benchmark** (`wiki/benchmarks/`).

- **Measurement-first:** o harness de recall@k reproduzível **existe** (`benchmarks/theodb_bench/`) e é
  pré-requisito de qualquer claim de performance. Estado medido: SIFT1M vs ScaNN (M33 — gap ~25× QPS) e vs
  pgvector hnsw (M45 — **paridade** recall×QPS). Nenhuma afirmação de performance sem artefato em `wiki/benchmarks/`.
- **VEREDITO MEDIDO FINAL do pilar vetorial (M73, 2026-07-10, `wiki/decisions/0035-m73-northstar-vector-verdict.md` + `wiki/benchmarks/m73-headtohead-verdict.md`):**
  **paridade own-code de recall classe-pgvector ALCANÇADA** (M60/M69/M70); **throughput multi-cliente
  competitivo-a-superior** vs pgvector no regime 128d clusterizado (M72, +11% QPS a recall casado); **superioridade
  de QPS vetorial sobre o ScaNN/AlloyDB MEDIDA como NÃO-ALCANÇÁVEL** por extensão PG permissiva (gap ~25-44× @ 0.99 é
  de paradigma — AH-LUT anisotrópico + não pagar o imposto MVCC/WAL; RaBitQ, melhor quantizador permissivo, dá
  memória não QPS — M74/ADR-0036). Posicionamento permitido: "paridade recall + memória billion-scale + AI-native/
  HTAP/aberto"; **jamais** "mais rápido que o AlloyDB no vetor". Reposicionamento formal do North Star: `wiki/decisions/0033-north-star-reposition-proposal.md`
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
   sem benchmark reproduzível e publicado em `wiki/benchmarks/` (`../.claude/rules/public-copy.md`).
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

Catálogo versionado: `.claude/knowledge-base/references-catalog.md` *(fora do versionamento)*
— é o contrato; toda referência nova entra lá. Os arquivos são gitignored (2,3 GB) e reprodutíveis
pelos comandos de clone do catálogo.

| Onde | O que tem | Use para |
|---|---|---|
| `.claude/knowledge-base/references/papers/` | **25 PDFs** — HNSW, ScaNN/AQ, RaBitQ, DiskANN, Faiss, IIR (livro), RRF, BEIR, C-Store, MonetDB/X100, Morsel, ARIES, SSI/MVCC, rigor estatístico, GraphRAG, prompt injection | teoria, matemática, e o SOTA contra o qual medimos |
| `.claude/knowledge-base/references/{pgrx,tantivy,faiss,hnswlib,datafusion,arrow-rs,postgres,…}` | **33 repos** de peers, `--depth 1`, grep-áveis | como o SOTA implementa de verdade |
| `.claude/knowledge-base/references/FlameGraph/` | ferramental de profiling | o trabalho de perf do colunar (o **M148**, que o motivou, está concluído) |

> **⚠️ Estado verificado em 2026-08-07: os diretórios desta tabela NÃO estão no disco.** `knowledge-base/`
> contém apenas as pastas de ciclo; não há `references/`, `papers/`, `FlameGraph/`, nem o
> `references-catalog.md` que esta seção chama de contrato — saíram do versionamento com o tooling `.claude/`
> e são reprodutíveis pelos comandos de clone do catálogo, que também saiu. **Enquanto isso não for repovoado,
> a ordem de fundamentação abaixo tem o primeiro degrau indisponível**: vá para a web (R0) e declare o
> conhecimento interno como tal, em vez de citar um `arquivo:linha` que não resolve — citação que não resolve
> não entra (Regra 8), e é pior que a ausência dela.

**Atalhos por assunto** (o mínimo a abrir antes de tocar no tema):

- **`unsafe` / FFI / pgrx** → `references/pgrx/` (medido 2026-07-30: **151** blocos `unsafe {`, **205** `unsafe fn`, 431 tokens no `theodb_rs` —
  o "384" que constava aqui não reproduz sob nenhuma definição; é a classe de defeito mais cara já encontrada em review — panic atravessando C, `TopMemoryContext`, MVCC do SPI).
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

## Acervo de conhecimento — a wiki (consultar ANTES, escrever DEPOIS — inquebrável)

Bundle: **`wiki/`** — **282 conceitos** em [Open Knowledge Format v0.2](https://github.com/google/open-knowledge-format),
um arquivo por conceito, links markdown formando um grafo, `type` como único campo obrigatório.
É a **única documentação viva do projeto**: a árvore `docs/` que o originou foi removida do repositório e
só existe no histórico git em `f7c7b93`, para onde todo campo `resource` aponta na forma `git:f7c7b93:docs/…`
(resolve com `git show`). Versionado desde `c7e6b7d`.

**Por que ele existe.** O conhecimento que este projeto pagou para aprender já estava escrito — em 67 arquivos de
memória, 110 blueprints, notas e mensagens de commit. Espalhado, ele **não morde no momento em que seria útil**.
Numa única sessão do M169, seis diagnósticos caíram por medição e **nenhum era novo em espécie**: todos tinham
precedente registrado em algum lugar que não disparou.

| `type` (nº medido no disco) | A pergunta que ele responde | Quando ler |
|---|---|---|
| `Measurement` (168) | "isso já foi medido?" | **antes de publicar qualquer número** — ele pode já existir |
| `Decision` (60) | "isso já foi decidido, e sob qual razão?" | antes de reabrir qualquer escolha de arquitetura |
| `Feature` (19) | "o que está de fato entregue, e com qual ressalva?" | antes de prometer capacidade, ou de otimizar o eixo errado |
| `Technology` (17) | "o que essa peça é **no contexto deste projeto**?" | antes de opinar sobre dependência ou peer |
| `Reference` (8) / `Guide` (8) / `Runbook` (1) | "como se faz, e como se diagnostica?" | antes de escrever procedimento novo |

**Honest-negatives não são um `type` — são uma propriedade transversal.** Hipóteses do próprio projeto
derrubadas por medição vivem dentro de `Measurement` e `Decision` (tag `honest-negative`), junto com
**retratações preservadas**: um veredito de superioridade que não sobreviveu a medição rigorosa, e números
invalidados por dados degenerados, mantidos com aviso no topo porque apagá-los esconderia que foram citados.
`wiki/index.md` lista os principais. **Antes de propor aposta técnica ou milestone, procure aqui primeiro.**

**Escrita obrigatória.** Vira conceito: todo número publicado em `wiki/benchmarks/**`; toda alegação minha
derrubada por medição; toda aposta medida e refutada; toda propriedade de plataforma aprendida por falha; todo
método que passou a ser exigido. **Atualize o conceito existente** — nunca crie um segundo arquivo para a mesma
classe. Registre a mudança em `wiki/log.md` — **corrigindo por acréscimo, nunca por sobrescrita**: o log já
carrega uma correção sobre si mesmo, porque a premissa reescrita havia baseado uma decisão.

**NÃO vira conceito** (e escrever enchimento para satisfazer o gate é pior que não escrever): rastro de execução
— isso é `knowledge-base/` do ciclo; decisão de arquitetura — isso é ADR em `wiki/decisions/`; bug corrigido sem lição
generalizável; **dado bruto de medição** — JSON/CSV/log vão para `benchmarks/artifacts/`, fora do bundle.

**Verificação — o que existe e foi executado.** O validador é o da skill `okf`:

```
node ~/.claude/skills/okf/okf-validate.mjs wiki          # exit 0 | 1 (erro) | 2 (uso)
node ~/.claude/skills/okf/okf-validate.mjs wiki --strict # eleva as recomendações v0.2 a gate
```

Ele checa frontmatter presente, `type` não-vazio, links conceito→conceito que resolvem, órfãos do grafo e
higiene de proveniência v0.2. **Executado em 2026-08-07: 282 conceitos, 0 erros, 0 warnings, 0 links
quebrados, 0 órfãos — conformante.**

**O que NÃO é verificado — dito com precisão, porque a versão anterior desta seção afirmava o contrário:**

- **Não existe** `.claude/scripts/check_okf.py`, e nunca houve.
- `hooks/stop-validation.sh` **não conhece OKF**. Ele valida TDD (warn), CHANGELOG (BLOQUEIA), secrets
  (BLOQUEIA) e honestidade do README (warn) — nada mais.
- `hooks/userpromptsubmit-inject.sh` **não injeta** ponteiro para o acervo.
- **Nenhum gate automático exige** conceito `Measurement` ao publicar número em `wiki/benchmarks/**`.

**Limite honesto:** nenhum hook prova que eu li, e nenhum bloqueia por não ter lido. Os gatilhos de leitura
são instruction-grade, como os degraus 2, 3 e 5 da parsimony ladder. Afirmar mecanização inexistente é o
mesmo `cobertura-alegada-sem-execucao` que o acervo documenta — e foi exatamente o defeito que esta seção
carregava até 2026-08-07.

**Bundle descontinuado (registro, não omissão).** Esta seção descrevia antes um bundle de aprendizado v0.1
com 60 conceitos (`Failure Mode` / `Technique` / `Invariant` / `Honest Negative`) em
`.claude/knowledge-base/okf/`, mais um contrato em `.claude/rules/okf-knowledge-base.md`. **Nenhum dos dois
está no disco** — saíram com o resto do tooling `.claude/` na limpeza de 2026-08. A wiki **não os substitui
integralmente**: ela documenta o produto, aquele bundle registrava o método de trabalho.

## Fluxo de trabalho

- Branch de trabalho: **`workspace`** (Regra 4 / `git-safety.md` § 1). O fluxo é
  `workspace ──PR──> develop ──PR + tag semver──> main`: toda mudança **nasce** em `workspace`; `develop`
  **integra** e nunca origina; `main` recebe só releases.
- Toda mudança visível entra no [`CHANGELOG.md`](./CHANGELOG.md) `[Unreleased]` (Regra 6).
- Commits **sem** trailer `Co-Authored-By` (política do projeto — `../.claude/rules/cycle-review.md`).
- Trabalho não-trivial passa pelos ciclos: `cycle-discover` → `cycle-plan` → `cycle-implement`
  → `cycle-code-quality` → `cycle-review` (ver `../.claude/rules/`).
- Decisões arquiteturais viram ADRs em `wiki/decisions/` (**60 ADRs**, do `0001-no-engine-fork` ao
  `0059-m169-fail-open-cobre-falha-de-spill`, incluindo D1–D7 formalizados em
  `wiki/decisions/0006-own-code-postgres-based-rust-go.md`); toda nova decisão de arquitetura abre um ADR.
