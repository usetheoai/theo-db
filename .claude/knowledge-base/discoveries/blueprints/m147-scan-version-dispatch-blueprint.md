# Blueprint: Version-dispatch de formato on-disk em index-AMs Rust — o padrão para refatorar `scan.rs` (M147)

**Slug:** `m147-scan-version-dispatch` · **Created:** 2026-07-24 · **Discovery plan:** `.claude/knowledge-base/discoveries/plans/m147-scan-version-dispatch-plan.md` (v1.1, SHIPPABLE_WITH_CAVEATS 89)

## Context

O M147 refatora três hotspots de `theodb_rs/src/am/scan.rs` (issue #170, consenso 5-pilares) **com comportamento byte-idêntico preservado**: (1) if-ladder de 5 versões IVF → enum lido uma vez (OCP); (2) 8 gather helpers `Vec` → `Result + ?`; (3) kernel Stage-1 `ah_score_block` compartilhado in-memory sem vazar o `codes_off` on-disk por-versão. A restrição inquebrável é a **ADR-2 do M145** (`theodb_rs/src/am/page/mod.rs:571`): corpos de decode on-disk permanecem separados — unificá-los arrisca misparse→data-loss.

Este blueprint investigou três referências para não reinventar (Rule 9): **pgvectorscale** (par pgrx, o padrão de dispatch OCP), **lance** (formato versionado Rust, o padrão de isolamento de corpos = a ADR-2), **pgvector** (o contraste "não versionar"). O escopo é engenharia-de-formato, não algoritmo ANN (o M147 não introduz técnica nova).

## Objective

Decidir a forma concreta do refactor: (a) o `enum IvfVersion`, (b) o contrato do kernel Stage-1 compartilhado, (c) o idioma `Result + ?` — cada um ancorado num padrão real citado.

---

## Coverage Corner 1 — Integration Tests

Como os projetos maduros provam "comportamento preservado" através de versões — o modelo para o A/B byte-idêntico do M147 (DoD bullet 4).

**pgvectorscale (Q4)** — scaffold "um corpo, N parametrizações" + asserção de identidade exata:

| Padrão | `path:line` (base `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/`) |
|---|---|
| Scaffold de scan parametrizado por `(distance, options, dims)` | `src/access_method/build.rs:1179` (`_bounded_memory` em `:1195`) |
| Determinismo via `setseed(0.5)` + 300 vetores | `src/access_method/build.rs:1219-1230` |
| Força index scan (`enable_seqscan=0`) | `src/access_method/build.rs:1235,:1257` |
| **Asserção = COUNT exato** (nenhuma tupla perdida) | `src/access_method/build.rs:1268` (`assert_eq!(cnt,300)`), `:1413` (`312` após inserts) |
| **Known-item — identidade exata do NN** | `src/access_method/build.rs:1419` (`test_no_rescore`); com rescore `ORDER BY <-> '[1,1,1]' LIMIT 1` retorna exatamente `[1,1,1]` (`:1467`); sem rescore o resultado é comprovadamente errado (`:1451`) — prova que o caminho importa |
| Mesmo scaffold cobre múltiplas distâncias/storages | `src/access_method/plain/tests.rs:80,:91,:101`; `sbq/tests.rs:9,:20,:32` |
| **Compat de formato antigo pós-upgrade** (o análogo direto do "v4..v7 ainda parseiam") | `src/access_method/upgrade_test.rs:196` (assert pré-upgrade `cnt=303`), `:238-239` (**mesma asserção pós `ALTER EXTENSION UPDATE`**) |

**lance (Q6)** — roundtrip parametrizado por versão + corpus versionado:

| Padrão | `path:line` (base `.claude/knowledge-base/references/lance/`) |
|---|---|
| **Roundtrip `#[rstest]` `#[values(V2_0, V2_1, V2_2)]`** — mesma asserção varrida por versão (o A/B byte-idêntico estrutural) | `rust/lance-file/src/reader.rs:2836-2844` (`test_projection`); idem `:2766`, `:3438`, `:3749` |
| Bench de leitura segregado por versão | `rust/lance-file/benches/reader.rs:29-58` (`for version in [V2_0,V2_1,V2_2]`) |
| Corpus de arquivos gerados por releases antigos, com assert de proveniência | `test_data/v1.0.1/datagen.py:26-27` (`assert lance.__version__ == "1.0.1"`) |
| Teste que lê dataset histórico com a build atual | `python/python/tests/test_backwards_compatibility.py:8-14` |
| CI de compat pareado | `.github/workflows/compat-pair.yml` |

**Aplicação ao M147:** o A/B byte-idêntico do DoD bullet 4 combina os dois padrões — (a) um scaffold único parametrizado pelas 5 versões IVF (como o `build.rs:1179` do pgvectorscale, e o `#[rstest]` do lance), (b) determinismo `setseed`, (c) asserção de **igualdade exata do resultado (ids + distâncias)** antes vs depois do refactor sobre um índice construído em cada versão. O `upgrade_test.rs:238` do pgvectorscale é o modelo para "índice de versão antiga ainda lê correto".

**Caveat honesto:** a suíte `upgrade_test` do pgvectorscale é `#[ignore]` e **pula PG18** (`upgrade_test.rs:40-43`) — o padrão transfere, a suíte não roda no nosso alvo. O nosso A/B roda in-PG no droplet (metodologia M145), não como `#[pg_test]`.

## Coverage Corner 2 — Dependencies

**pgvectorscale (Q5)** — pgrx 0.16.1 vs nosso 0.19:

| Fato | `path:line` |
|---|---|
| pgrx pinado em `=0.16.1` (nós: 0.19, Δ 3 minors) | `Cargo.toml:31,:42-43` |
| Callbacks já usam `extern "C-unwind"` + `PgBox<IndexScanDescData>` — mesmo idioma do 0.19 | `src/access_method/scan.rs:308,:335,:369,:438` |
| `Result + ?` é **interno Rust-side, não constrangido pela versão pgrx** — o callback C retorna `bool`/ponteiro; a conversão dos gathers é acima do callback | `src/access_method/scan.rs:210` (`next<S:Storage>` retorna `Option`), `:407` (tradução para `bool` no callback) |
| **NÃO transferir** os idiomas Spi 0.16 (`DatumWithOid::new` + `get_one_with_args`) — mudaram para 0.19 | `src/access_method/build.rs:1263` |

**Conclusão para o M147:** o bullet 2 (`Result + ?`) não depende da versão do pgrx — é refactor interno acima da fronteira C. Zero risco de ABI. O único delta é de idiomas de teste (Spi), que não tocamos no refactor.

## Coverage Corner 3 — Tools

Coberto junto do Corner 1 (o tooling de prova de compat É o A/B): scaffold parametrizado (`build.rs:1179`), `#[rstest]` por versão (`reader.rs:2836`), bench por versão (`benches/reader.rs:29`), corpus versionado (`test_data/`). O análogo TheoDB é o A/B in-PG do M145 rodado no droplet — não há tooling novo a criar (parsimony rung 1): reusa-se a metodologia M145.

## Coverage Corner 4 — Techniques

O núcleo do blueprint: como despachar por versão (bullet 1) e isolar corpos (a ADR-2).

### O padrão de dispatch OCP (pgvectorscale, Q1)

| Elemento | `path:line` |
|---|---|
| `trait Storage` — contrato de comportamento (14 métodos) | `src/access_method/storage.rs:41-141` |
| `enum StorageType { Plain=0, SbqCompression=2 }` com **discriminante retirado preservado** (`// R.I.P. SbqSpeedup = 1`, nunca renumerado) | `src/access_method/storage.rs:145-147` |
| Tipo **lido UMA vez** de `meta_page.get_storage_type()` → despachado | `src/access_method/scan.rs:65` (lê) → `:68` (`match storage {...}`) |
| Kernel de busca **genérico sobre `S: Storage`**, escrito uma vez, nunca tocado ao adicionar tipo | `src/access_method/scan.rs:210` (`next<S: Storage>`) |
| Decode on-disk **isolado por-impl** (Plain vs SBQ separados) | `src/access_method/plain/storage.rs:112`, `src/access_method/sbq/storage.rs:237` |
| Matches exaustivos (sem `_ =>`) → compilador força atualizar cada sítio | `scan.rs:68,:381,:451`; `meta_page.rs:278,:287` |

**Honestidade estrutural:** pgvectorscale é grafo DiskANN, não IVF, e despacha por 2 storage-types, não 5 versões on-disk. Transfere o **padrão** (tipo lido uma vez + kernel genérico + decode isolado), não uma receita 1:1. Adicionar um tipo lá é "1 impl + 1 variante + ~6 arms de match" — **OCP parcial compiler-enforced**, não OCP puro.

### O padrão de isolamento de corpos = a ADR-2 na prática (lance, Q2)

| Elemento | `path:line` (base `.claude/knowledge-base/references/lance/`) |
|---|---|
| Bytes de versão lidos UMA vez no footer | `rust/lance-file/src/reader.rs:794-795` |
| **Bytes→enum centralizado numa função** (`try_from_major_minor`) | `rust/lance-encoding/src/version.rs:62-76` |
| `enum LanceFileVersion` derivando `Ord` → gates `version >= V2_1` | `rust/lance-encoding/src/version.rs:18-40`; uso em `reader.rs:323,:1185` |
| **Guard fail-closed: o reader v2 recusa arquivo v1** | `rust/lance-file/src/reader.rs:797-803` (`version_conflict`) |
| **Corpo legado isolado em módulo `previous/`** (autocontido, format/reader/writer próprios) | `rust/lance-file/src/previous/mod.rs:4-9` |
| Escolha barata do reader por bytes de versão, **sem reabrir o bloco** | `rust/lance-table/src/format/fragment.rs:183-185` (`is_legacy_file()`) |
| **Dispatch trait-object UMA vez na abertura** (`Box<dyn GenericFileReader>`) | `rust/lance/src/dataset/fragment.rs:1040-1107` |
| Sub-versões compatíveis (2.0↔2.1) dividem caminho via `match` ordinal dentro do reader v2 | `rust/lance-file/src/reader.rs:1168-1187` |

**A regra que o lance ensina (e que é exatamente a ADR-2):** famílias de formato que **arriscam misparse** (v1 vs v2) ficam em **módulos fisicamente separados** com recusa fail-closed cruzada; **variações compatíveis contíguas** (2.0↔2.1, mesmo protobuf) podem dividir caminho via `match` ordinal. A única coisa unificada é a **leitura do discriminante**, não o corpo de decode.

**Nuance honesta:** o lance tem DUAS tabelas bytes→enum (`version.rs:62` com `Result`, e `reader.rs:233-244` com `panic!` no default). Ao portar, unificar a leitura do discriminante numa única função — o M147 não deve repetir a duplicação.

### O contraste "não versionar" (pgvector, Q3)

| Fato | `path:line` (base `.claude/knowledge-base/references/pgvector/`) |
|---|---|
| `IVFFLAT_VERSION 1` + campo `version` no metapage | `src/ivfflat.h:42,:237` |
| Abertura valida SÓ o magic, **não lê a versão** | `src/ivfutils.c:215` (`elog(ERROR, "ivfflat index is not valid")`) |
| `ivfflatgettuple` — **um único caminho, sem dispatch por versão** | `src/ivfscan.c:354-395` |

**Lição:** pgvector é single-version + reindex-on-upgrade — "resolve" o problema não o tendo. O TheoDB fez a escolha oposta e deliberada (6 versões para ler índices antigos sem reindex), o que é a fonte da if-ladder. Isso **reforça a ADR-2**: já que versionamos, o padrão certo é lance+pgvectorscale, não pgvector. **NÃO transferir** o `elog(ERROR)` genérico — o TheoDB distingue XX002/22023 desde o M146.

---

## Cross-cutting Comparison

| Dimensão | pgvectorscale | lance | pgvector | Decisão M147 |
|---|---|---|---|---|
| Dispatch de versão/tipo | `match` sobre `enum StorageType`, tipo lido uma vez (`scan.rs:65→68`) | `try_from_major_minor` (bytes→enum uma vez) + `Box<dyn Reader>` na abertura | nenhum (single-version) | **enum `IvfVersion` lido uma vez** do bloco 0, substituindo a if-ladder de 5 releituras |
| Isolamento de corpos on-disk | `impl Storage` separadas (`plain/` vs `sbq/`) | módulo `previous/` + recusa fail-closed | n/a | **corpos `scan_ivf_aq_*` permanecem separados** (ADR-2); só o kernel Stage-1 in-memory é hoistado |
| Kernel compartilhado | `next<S: Storage>` genérico | reader v2 compartilha caminho entre sub-versões compatíveis | único caminho | **kernel Stage-1 recebe `codes_off` como parâmetro** — nunca recomputa (protege ADR-2) |
| Idioma de erro | `panic!`/`assert!` (NÃO transferir) | `Result` (`version.rs:62`) + `panic!` (`reader.rs:233`, duplicado) | `elog(ERROR)` genérico | **`Result + ?` + `err_input`/`err_corrupt` tipado** (idioma M146) |
| Prova de compat | scaffold `build.rs:1179` + `upgrade_test.rs:238` (`#[ignore]`, pula PG18) | `#[rstest]` por versão + corpus `test_data/` | reindex (não prova) | **A/B byte-idêntico in-PG (M145)** sobre índice construído em cada versão v4..v8 |
| Discriminante retirado | preservado (`R.I.P. SbqSpeedup=1`) | `Legacy` como variante ordinal | n/a | **enum preserva v3..v8**, discriminante = o `u32` já persistido em `[4..8]` |

## ADRs (decisões da síntese)

### D1 — O enum `IvfVersion` lê o `u32` já persistido, não inventa discriminante novo
O TheoDB já grava o discriminante de versão em bytes `[4..8]` do bloco 0 (`page/ivf.rs`, magic `TIVS` compartilhado + `u32` version). O enum mapeia esse `u32` existente (v3=3..v8=8) — **zero mudança de formato on-disk** (crash/upgrade-safety preservados por construção). Alternativa rejeitada: magic distinto por versão (mudaria o formato → exigiria upgrade). Modelo: `lance version.rs:62` (bytes→enum centralizado), pgvectorscale `StorageType` (discriminante estável).

### D2 — Corpos de decode ficam separados; só o kernel Stage-1 in-memory é compartilhado
Segue a ADR-2 + o padrão lance (`previous/` isolado) + pgvectorscale (`impl Storage` por-tipo). O kernel compartilhado recebe `codes_off` (e a política inline-tid/inline-label) como **parâmetro do chamador**, nunca recomputa — porque `codes_off` é conhecimento on-disk por-versão (v4 `8n+entry_f32·n`; v5/v6/v8 `8n`; v7 `8n+n·label_bytes`, mapeado pelo agente de baseline). Alternativa rejeitada: kernel que recomputa `codes_off` internamente → reintroduz o risco misparse que a ADR-2 barra.

### D3 — `Result + ?` é refactor interno acima da fronteira C
Provado independente da versão pgrx (Q5): os callbacks C retornam `bool`/ponteiro; a conversão dos 8 gathers `Vec → Result` + `?` acontece Rust-side, com o boundary de erro (`err_corrupt`/`err_input`/`lut_error`) hoistado para os call-sites do `amrescan`. Modelo: pgvectorscale `next<S>` retorna `Option` (`scan.rs:210`), callback traduz (`:407`).

## Recommendations (mapeadas aos 3 bullets do DoD)

1. **Bullet 1 (if-ladder → enum):** definir `enum IvfVersion { V3, V4, V5, V6, V7, V8 }` com uma única função `IvfVersion::from_block0(rel) -> Result<IvfVersion, String>` que lê o bloco 0 **uma vez** e mapeia o `u32` de `[4..8]` — substituindo as 5 leituras redundantes de `ivf_is_v4/…/v8`. O dispatch vira um `match version { … }` (exaustivo, sem `_ =>`, como pgvectorscale). Ancora: `lance version.rs:62`, `pgvectorscale scan.rs:68`.
2. **Bullet 3 (Stage-1 compartilhado):** extrair `fn stage1_score(lut, bytes, codes_off, n, pairs, out) -> Vec<Candidate>` — o loop de bloco + `ah_score_block`, **recebendo `codes_off` do chamador**. Cada corpo `scan_ivf_aq_*` calcula seu `codes_off` (decode on-disk, permanece separado) e chama o kernel. Ancora: pgvectorscale `next<S: Storage>` genérico + decode isolado por-impl; ADR-2.
3. **Bullet 2 (`Result + ?`):** mudar as 8 assinaturas para `Result<Vec<(i64,f64)>, ScanError>`, converter os ~46 `match { Ok=>v, Err=>err_* }` em `?`, e hoistar a conversão-para-ereport (`err_corrupt`/`err_input`/`lut_error`) para os 3 call-sites de dispatch no `amrescan`. Ancora: D3.

## Padrões que NÃO se aplicam (honestidade)

- **pgvectorscale `panic!`/`assert!` em produção** (`storage.rs:158,:166`; `scan.rs:343`) — o TheoDB usa erro tipado desde o M146; copiar seria regressão XX000.
- **pgvector `elog(ERROR)` genérico** — perde a distinção XX002/22023.
- **lance duplicar a tabela bytes→enum** (`reader.rs:233` além de `version.rs:62`) — o M147 unifica a leitura numa função.
- **pgvectorscale idiomas Spi 0.16** (`DatumWithOid`, `get_one_with_args`) — reescrever para 0.19 se tocados (mas o refactor não toca Spi).
- **pgvectorscale `upgrade_test` suite** — `#[ignore]` + pula PG18; o padrão transfere, a suíte não roda no nosso alvo. O A/B roda in-PG no droplet.

## Referências

- pgvectorscale: `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/{storage.rs,scan.rs,meta_page.rs,plain/storage.rs,sbq/storage.rs,build.rs,upgrade_test.rs,plain/tests.rs,sbq/tests.rs}`, `Cargo.toml`
- lance: `.claude/knowledge-base/references/lance/rust/lance-file/src/{reader.rs,format.rs,previous/mod.rs}`, `rust/lance-encoding/src/version.rs`, `rust/lance-table/src/format/fragment.rs`, `rust/lance/src/dataset/fragment.rs`, `rust/lance-file/benches/reader.rs`, `python/python/tests/test_backwards_compatibility.py`
- pgvector: `.claude/knowledge-base/references/pgvector/src/{ivfscan.c,ivfutils.c,ivfflat.h}`
- Baseline TheoDB: `theodb_rs/src/am/scan.rs`, `theodb_rs/src/am/page/ivf.rs`, `theodb_rs/src/vec/ah.rs` (mapeado pelo agente de baseline — a fronteira decode-on-disk vs scoring-in-memory por corpo)
- Restrição: ADR-2 do M145 (`theodb_rs/src/am/page/mod.rs:571`), `.claude/rules/parsimony-ladder.md`, `.claude/rules/error-handling.md`
