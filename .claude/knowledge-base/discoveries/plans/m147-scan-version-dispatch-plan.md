# Discovery Plan: Version-dispatch de formato on-disk em index-AMs Rust — como refatorar `scan.rs` sem unificar corpos

> **Version 1.1** — Esta discovery investiga como três index-AMs / formatos-de-arquivo maduros (pgvectorscale, o par pgrx mais próximo; lance, formato colunar versionado em Rust; pgvector, o AM de referência em C) resolvem **dispatch de versão de formato on-disk de forma OCP** e **isolamento dos corpos de decode por-versão sem duplicação acidental**. O output é um blueprint que decide a forma concreta do refactor do M147 (`am/scan.rs`): a estrutura do enum de versão (bullet 1), o idioma `Result + ?` (bullet 2), e — o mais arriscado — como hoistar o kernel Stage-1 in-memory compartilhado (bullet 3) **sem** recomputar o `codes_off` que é conhecimento on-disk por-versão (ADR-2 do M145). Escopo deliberadamente de engenharia-de-formato, não de algoritmo ANN: o M147 preserva comportamento byte-idêntico e não introduz técnica nova.

**Slug:** `m147-scan-version-dispatch`
**Owner:** paulohenriquevn
**Created:** 2026-07-24 · **Revised:** 2026-07-24 (v1.1 — MUST-FIX EC-1/EC-2 do edge-cases absorvidos: Q1 aponta o dispatch real em scan.rs:68; Q4 aponta os testes de scan em mod.rs)
**Time budget:** 6h (per-project em ADR D1)

## Context

O issue #170 é um consenso de 5 pilares (code, architecture, idiomaticity, design_patterns, maintainability) sobre um hotspot em `theodb_rs/src/am/scan.rs`: uma if-ladder de versão (`scan.rs:545-563`) que emite até 5 leituras redundantes do bloco 0, ~46 `match { Ok=>v, Err=>err_corrupt }` C-style em 8 gather helpers, e um kernel Stage-1 (`ah_score_block`) copiado byte-a-byte em 5 corpos `scan_ivf_aq_*`. O M147 refatora esses três pontos **com comportamento preservado**.

A restrição que domina o design é a **ADR-2 do M145** (registrada em `theodb_rs/src/am/page/mod.rs:571`): os corpos de formato on-disk permanecem separados — offsets/strides por-versão são complexidade essencial, e unificá-los arrisca misparse→data-loss. O `.claude/rules/parsimony-ladder.md` reforça: per-version = essencial, não acidental; o anti-sunk-cost não se aplica a corretude. Portanto o refactor pode compartilhar o kernel **in-memory** (Stage-1 scoring) mas nunca o **decode de bytes** (Stage-2 e o cálculo de `codes_off`).

A pergunta que esta discovery fecha antes do `/to-plan`: qual é a forma comprovada, em projetos maduros, de (a) dispatch de versão de formato OCP-compliant, (b) isolar corpos de decode por-versão, e (c) hoistar lógica compartilhada in-memory sem vazar conhecimento on-disk — para não reinventar (Rule 9) e não cair na tentação de unificar corpos (o risco (b) do DoD).

Regras do projeto que qualquer padrão emprestado respeita: `.claude/rules/architecture.md` (fronteiras domain/adapter — o decode on-disk é adapter, o scoring é domain), `.claude/rules/testing.md` (edge vs negative cases — todo corpo de decode precisa de teste de borda de versão), `.claude/rules/error-handling.md` (typed errors — o `Result + ?` do bullet 2 é exatamente fail-fast tipado).

## Objective

O blueprint deve permitir decidir a **forma concreta** do refactor do M147: a definição do `enum IvfVersion`, o contrato do kernel Stage-1 compartilhado (quais parâmetros o chamador injeta), e o idioma `Result + ?` — cada um ancorado num padrão real de projeto maduro, não numa invenção.

- [ ] Todas as research questions respondidas com citações a `.claude/knowledge-base/references/`
- [ ] Tabela de comparação cross-cutting populada para pgvectorscale, lance e pgvector
- [ ] Recommendations com ≥ 1 proposta de decisão concreta por research question, mapeada a um dos 3 bullets do DoD
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/src/access_method/` (esp. `storage.rs`, `sbq/`, `plain/`, `scan.rs`) | Par pgrx mais próximo (mesmo modelo de extensão); tem o padrão `trait Storage` + `StorageType` dispatch — o alvo OCP do bullet 1 |
| `.claude/knowledge-base/references/lance/` | `rust/lance-file/src/` (esp. `reader.rs`, `format.rs`, `previous/`) | Formato de arquivo colunar VERSIONADO em Rust; `previous/` isola versões antigas — a ADR-2 na prática |
| `.claude/knowledge-base/references/pgvector/` | `src/ivfscan.c`, `src/ivfutils.c` | AM IVF de referência em C; baseline de como o dispatch de scan é feito sem enum (contraste com o alvo Rust) |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/lance/python/`, `lance/java/` | Bindings não-Rust; o padrão que importa é o do core Rust |
| `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/build.rs`, `vacuum.rs` | Build/vacuum não são o caminho de scan que o M147 refatora |
| Os arquivos `hnsw*.c` do pgvector (não listados como path aqui de propósito — ver nota) | O M147 é sobre o dispatch IVF/AQ; o HNSW do pgvector não tem a if-ladder de 5 versões que este refactor ataca |
| Qualquer projeto NÃO clonado em `.claude/knowledge-base/references/` | Cross-Project Rule: nunca alegar feature sem ler a fonte |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvectorscale: 3h (o padrão OCP central + par pgrx); lance: 2h (o isolamento de versão); pgvector: 1h (só o contraste C).

**Rationale:** pgvectorscale é o análogo mais próximo (pgrx, index-AM, `trait Storage`) e responde os bullets 1 e 3 diretamente; lance responde o bullet 1 (dispatch por versão lido do header) e a ADR-2 (isolamento de corpos); pgvector é contraste informativo (como se faz sem enum). Deep dive proporcional à proximidade.

**Alternatives considered:** split igual (rejeitado — pgvector agrega menos por ser C e não ter a if-ladder de 5 versões); só pgvectorscale (rejeitado — perde o padrão de isolamento de versão de formato do lance, que é exatamente a ADR-2).

**Stop condition — per question (mandatory):** Quando a Fase A de uma questão retorna zero matches após 3 retries com variantes de query (pattern → kind-based → path alternativo → escopo mais amplo), marca BLOCKED com "Fase A exhausted — no hotspots" e continua. NÃO preenche com hotspots de outra questão.

**Stop condition — per project (mandatory):** Quando o budget de um projeto esgota com questões pendentes, marca as restantes daquele projeto BLOCKED com "budget exhausted" e passa ao próximo. Se todo projeto restante estiver nesse estado (toda questão `done` ou honestamente `blocked`), emite `<promise>BLUEPRINT_BLOCKED</promise>` com o relatório honesto — nunca `BLUEPRINT_COMPLETE` a partir de estado com questões blocked.

**Anti-pattern:** NUNCA fabricar respostas de Fase B para fechar uma questão cuja Fase A esgotou. BLOCKED honesto com razão é obrigatório (Rule 3).

**Consequences:** o halt-loop para de iterar num projeto quando o budget esgota; o blueprint expõe questões blocked na seção própria — viram semente da próxima discovery.

### D2 — Investigation depth

**Decision:** Fase A ast-grep para mapear os hotspots de dispatch/trait/version-match; Fase B Read end-to-end de cada corpo de decode e de cada `impl` de trait, capturando os comentários de intenção (é onde a razão do isolamento por-versão está documentada).

**Rationale:** o valor está nos comentários que explicam POR QUE cada versão é isolada — grep sozinho perde isso. Alternativa (só grep de símbolos) rejeitada: perderia a justificativa de design que é o núcleo desta discovery.

**Consequences:** trade-off explícito — Read end-to-end é mais lento, cabe no budget porque o escopo é cirúrgico (3 arquivos por projeto, não a árvore toda).

### D3 — Escopo é engenharia-de-formato, não algoritmo (deferral de SOTA de ANN)

**Decision:** Esta discovery NÃO investiga SOTA de algoritmo ANN (nem AlloyDB/ScaNN, nem papers de quantização). Fica em como projetos maduros estruturam dispatch de versão de formato e isolam corpos de decode.

**Rationale:** o M147 preserva comportamento byte-idêntico e não introduz técnica nova (é refactor de OCP). O bar de PhD-rigor (`.claude/rules/discover-phd-rigor.md § 1`) aplica-se a topics algorithm-bearing; um SOTA de ANN aqui seria deep-research theatre para um refactor. O foco é Rule 9 (não reinventar o padrão de version-dispatch) e a ADR-2 (não unificar corpos).

**Consequences:** o R0 do phd-rigor (busca web) é satisfeito por evidência de código-fonte primário nas referências clonadas + o padrão OCP na literatura de design (GoF Strategy / open-closed), não por papers de arXiv. Se o `/to-plan` posterior precisar de evidência web adicional, será um follow-up.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad — ast-grep map) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | Como o pgvectorscale despacha entre tipos de storage (sbq/plain) de forma OCP — trait + enum — e ONDE o dispatch de comportamento é feito (o match que escolhe a impl)? | techniques | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/` | `ast-grep -p 'trait Storage { $$$ }'` + `-p 'impl Storage for $T { $$$ }'` em `storage.rs`, `sbq/storage.rs`, `plain/storage.rs`; **E `-p 'match $S { StorageType::$V => $$$ }'` em `scan.rs` + `meta_page.rs`** — o dispatch de COMPORTAMENTO não está em `storage.rs` (só a conversão u8→enum), está em `scan.rs:68` e `meta_page.rs:278` (EC-1) | Read `storage.rs:41` (trait) + `:145-169` (conversão), cada `impl Storage`, **e o `match storage` de `scan.rs:68`**; capturar como um tipo novo entra (add impl + arm) | Tabela: trait method → por-tipo impl → o ponto de dispatch (`scan.rs:68`), com `path:line` |
| Q2 | Como o lance despacha a leitura por VERSÃO de formato de arquivo (lê a versão do header uma vez) e isola os corpos de versões antigas? | techniques | `.claude/knowledge-base/references/lance/rust/lance-file/src/` | `ast-grep run -p 'match ($$$) { $$$ }' --lang rust` em `reader.rs` (o `match (major, minor)`); `ls previous/` para o isolamento | Read `reader.rs:232-240` (o version match) e `previous/mod.rs` + `previous/reader.rs`; capturar a fronteira "ler versão uma vez → despachar para o corpo isolado" | Descrição do dispatch por versão + como `previous/` isola corpos, com citações |
| Q3 | Como o pgvector (C) despacha o scan IVF sem enum — o baseline de contraste? | techniques | `.claude/knowledge-base/references/pgvector/src/` | SKIP Fase A (C, fora do ast-grep rust); Grep `IvfflatScanOpaque`/`ivfflatgettuple`/`GetScanValue` em `ivfscan.c` | Read `ivfscan.c` no dispatch de scan; capturar como o formato é lido e se há versionamento inline | Descrição de como um AM C evita/faz version-dispatch, contraste com o alvo Rust do M147 |
| Q4 | Como o pgvectorscale testa que um scan retorna o resultado correto — o padrão de teste que o M147 precisa para provar "comportamento preservado"? | tests | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/` | `ast-grep -p '#[pg_test] fn $NAME() { $$$ }'` em **`mod.rs`** (onde vivem os `#[pg_test]` que asseveram RESULTADO de scan — `sbq/tests.rs`/`plain/tests.rs` só testam build/vacuum, sem `ORDER BY`/`assert_eq` — EC-2) + `upgrade_test.rs` para o padrão de compat de versão antiga | Read cada `#[pg_test]` de scan em `mod.rs` + os de `upgrade_test.rs`; capturar como asseveram recall/ordem e como testam compat de versão antiga | Tabela: teste → o que asservera → como monta o índice, com `path:line` |
| Q5 | Que versão de pgrx o pgvectorscale usa e como isso difere do nosso 0.19 no que toca a assinaturas de scan (`amgettuple`/`amrescan`)? | deps | `.claude/knowledge-base/references/pgvectorscale/` | SKIP Fase A (text-shape); Grep `pgrx` em `pgvectorscale/Cargo.toml` + as assinaturas em `scan.rs` | Read `Cargo.toml` (versão pinada) + `scan.rs` (assinaturas de callback); capturar deltas relevantes ao nosso refactor | Versão pgrx + deltas de assinatura que afetam o refactor |
| Q6 | Que tooling o lance-file usa para provar compat de versão de formato (bench/roundtrip) — o análogo do nosso A/B byte-idêntico? | tools | `.claude/knowledge-base/references/lance/rust/lance-file/` | SKIP Fase A (Glob); `ls benches/`, `ls src/previous/`, Grep `roundtrip`/`backwards` em `reader.rs` | Read `benches/reader.rs` + qualquer teste de roundtrip de versão; capturar como medem que uma versão antiga ainda lê | Descrição da estratégia de teste de compat de versão + citações |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q4 | Covered |
| Dependencies | Q5 | Covered |
| Tools | Q6 | Covered |
| Techniques | Q1, Q2, Q3 | Covered |

**Coverage: 4/4 corners covered (100%)**

O corner Techniques carrega 3 questões (Q1, Q2, Q3) — acima do mínimo de 2 do phd-rigor R4 — porque é onde vivem as apostas do M147 (o dispatch OCP e o isolamento de corpos são o núcleo do refactor).

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | O `path` declarado na Fase A de Qx existe | Marca Qx BLOCKED "path not found", continua |
| Per-question Fase A budget | Fase A retornou ≥ 1 hotspot OU 3 retries de variante tentados | Após 3 retries vazios, Qx BLOCKED "Fase A exhausted"; continua |
| After answering Qx | A seção do blueprint sob Qx tem ≥ 1 citação | Re-itera Qx (1 retry) |
| Mid-loop sanity | Citações a `.claude/knowledge-base/references/` ≥ N / 200 palavras de prosa | Adiciona citações aos parágrafos sub-citados (1 retry) |
| Per-project time budget | Budget do projeto não esgotado | Ao esgotar, questões restantes do projeto BLOCKED "budget exhausted"; avança |
| Before promising complete | Os 4 corners têm seção populada | Recusa a promessa, continua iterando |

## Acceptance Criteria

- [ ] Todas as research questions respondidas OU explicitamente BLOCKED com razão
- [ ] Toda citação aponta para um `path:line` real em `.claude/knowledge-base/references/`
- [ ] A tabela cross-cutting compara pgvectorscale × lance × pgvector nas dimensões: dispatch-de-versão, isolamento-de-corpos, idioma-de-erro, teste-de-compat
- [ ] Recommendations propõe: (a) a forma do `enum IvfVersion` (bullet 1), (b) o contrato do kernel Stage-1 compartilhado — quais params o chamador injeta (bullet 3), (c) o idioma `Result + ?` (bullet 2) — cada um ancorado num padrão real citado
- [ ] O blueprint declara explicitamente qual padrão das referências **não** se aplica e por quê (honestidade — nem tudo transfere)

## Global Definition of Done

- Blueprint em `.claude/knowledge-base/discoveries/blueprints/m147-scan-version-dispatch-blueprint.md`
- `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS (thresholds em `.claude/rules/discover-blueprint-thresholds.txt`; golden rule `.claude/rules/discover-blueprint-golden-rule.md`)
- Zero citação fabricada (hard cap do golden rule)
- Os 4 coverage corners populados (hard cap)
