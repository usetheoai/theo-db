---
slug: m169-scale-bugs-100m
target_project: theo-db
milestone_id: M169
created_at: 2026-07-29
goal: Fazer o q20 do ClickBench completar sem erro a 100M linhas, eliminando o `byte array offset overflow` do caminho agregado.
---

# M169 — bugs de escala a 100M

## Goal

**Fazer o q20 do ClickBench completar sem erro a 100M linhas** — métrica observável única: `q20` sai de
`byte array offset overflow` para `rc=0` com resultado byte-idêntico ao heap.

Não é trabalho de performance. O critério é *a consulta termina*, não *a consulta é rápida* (mandato do owner,
2026-07-29: "a performance atual basta; bugs de escala não").

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Último commit | Por que existe |
|---|---|---|---|
| `theodb_rs/src/am/df_executor.rs` | **1772** | `3775d70` (2026-07-29) | ponte entre o colunar e o DataFusion: decode → Arrow → plano → Datums |
| `theodb_rs/src/am/columnar.rs` | **2654** | `05b5e6a` (2026-07-29) | o TableAM colunar: stripes, chunk-groups, zone-map, `ColumnarChunkStream` |

**Budget de LoC:** ambos já excedem o default de 500 de `architecture.md`. Este plano **não os aumenta
materialmente** — a mudança central é trocar uma chamada por outra nos dois call-sites, reusando função existente.
Qualquer crescimento acima de ~40 linhas por arquivo é sinal de que o escopo escorregou.

### Os dois call-sites alvo, com evidência

| Local | Função | O que faz hoje |
|---|---|---|
| `df_executor.rs:611` | `run_columnar_aggs` (`:601`) | `decode_to_batch(rel, &agg_cols, …)` — decodifica a **relação inteira** |
| `df_executor.rs:747` | `run_columnar_grouped_aggs` (`:719`) | `decode_to_batch(rel, &proj_cols, …)` — idem |

`decode_to_batch` (`:445`) chama `decode_columns_v2` (`:486`), que acumula buffers de **todos** os chunk-groups.

### Current callers / dependents

| Símbolo | Local | Chamadores hoje |
|---|---|---|
| `open_streaming_source` | `df_executor.rs:1088` | **um só**: `:1194`, dentro de `run_columnar_topk` |
| `run_df_collect_streaming` | `df_executor.rs:1275` | um só, o mesmo |
| `ColumnarChunkStream` | `columnar.rs:1118` | um só ponto de construção (`df_executor.rs:1129`) |
| `run_aggs_on_batch` | `df_executor.rs:619` | **TRÊS, e dois são fora do caminho colunar**: `:613` (`run_columnar_aggs`), `arrow_cache.rs:199` e `arrow_cache.rs:265` |

O terceiro chamador de `decode_to_batch` (`:1256`) é o **fallback eager** do próprio top-k — fica intocado, é a
rede de segurança.

**Restrição de desenho que essa última linha impõe (verificada por grep 2026-07-29).** `run_aggs_on_batch` é
`pub(super)` e serve também o caminho **heap-autoritativo do M101** (`arrow_cache.rs`), que nada tem de colunar.
Trocar o decode ali dentro contaminaria o cache Arrow do heap, e **nenhum teste deste milestone perceberia** — os
gates do M169 são todos sobre tabelas colunares. Portanto o T2.1 acrescenta um **irmão** streaming e deixa
`run_aggs_on_batch` com assinatura e corpo intactos. Isto não é preferência de estilo: é a diferença entre a
troca ser local e ser um refactor de dois caminhos.

### A falha do q20 a 100M é EMPÍRICA, não inferida

O blueprint fecha a cadeia causal por leitura de código, e eu descrevi a falha como "não observada". **Errado — ela
está registrada em artefato cru desde 2026-07-26** (achado do SEPA, verificado):

```
docs/benchmarks/m162-artifacts/theodb-100m-partial.jsonl:21
{"q": 20, "node": "PUSHDOWN:Custom Scan (theodb_columnar_agg)", "cold_s": 92.199,
 "hot_s": null, "status": "ERR:byte array offset overflow"}
```

Prosa correspondente em `docs/benchmarks/m162-100m-gap-verdict.md:35` ("*a real i32-offset scale bug*"). O que é
leitura de código é **apenas a cadeia interna** até `generic_bytes_builder.rs:87`; o fato — a mensagem, na escala,
no nó de pushdown, com o tempo até falhar — é medido.

**A aritmética também fecha com folga.** O q20 é `SELECT COUNT(*) FROM hits WHERE URL LIKE '%google%'`
(`benchmarks/clickbench/theodb/queries.sql:21`), e `URL TEXT NOT NULL`. Os offsets de um único `StringArray`
estouram acima de `i32::MAX = 2.147.483.647` bytes cumulativos; sobre 99.997.497 linhas isso é **21,5 bytes/linha
em média**. Qualquer corpus de URLs está ordens de grandeza acima. E nada pode evitar o decode: predicado de texto
**nunca** dirige skip de zone-map no caminho eager (`df_executor.rs:466-467` — só `predicates` chega ao
`decode_columns_v2`), então as ~100M células de `URL` são decodificadas sempre.

**Consequência para o escopo:** a Fase 1 continua obrigatória por ADR-2, mas ela mede **quantas das 43 completam**,
não **se o q20 falha** — isso já se sabe. O ramo honest-negative do grafo de dependências permanece, porém agora
só se aciona se o baseline mostrar o q20 passando, o que contradiria o artefato.

### Domain glossary

| Termo | Definição |
|---|---|
| **chunk-group** | unidade de decode do colunar: `CHUNK_GROUP_ROWS = 10_000` (`columnar_codec.rs:24`). 1M linhas = 100 |
| **caminho agregado** | `run_columnar_aggs` + `run_columnar_grouped_aggs` — o `CustomScan` `theodb_columnar_agg` |
| **caminho eager** | decodifica a relação inteira num `RecordBatch` antes de entregar ao DataFusion |
| **caminho streaming** | entrega um `RecordBatch` por chunk-group via `PartitionStream` (entregue no M168) |
| **teto `i32`** | offsets de `DataType::Utf8` do Arrow são `i32` → 2 GB por array |
| **spill** | derrame da tabela hash do agregado para disco quando a `MemoryPool` satura |

### Architecture boundaries affected

Nenhuma nova. A mudança é **intra-módulo** (`am/`), trocando uma função interna por outra do mesmo módulo. Não
cria export público, não inverte dependência, não atravessa camada. Por `architecture.md § 2`, não há DIP a
declarar aqui.

### O gate obrigatório que esta mudança dispara

`testing.md § 5.1` é explícito: **qualquer** mudança nos admit-paths do roteamento colunar tem de passar
`benchmarks/columnar_type_ab.py` **antes** do `/review`. Este plano muda o *executor*, não o admit — mas o
executor decide o tipo Arrow de saída, e o §5.1 existe porque o A/B do ClickBench **não exercita o espaço de
tipos**. Ele entra no DoD como gate duro.

## Prior Art & Related Work

| Fonte | O que ela dá |
|---|---|
| `.claude/knowledge-base/discoveries/blueprints/m169-scale-bugs-100m-blueprint.md` | a cadeia causal fechada até o `expect("byte array offset overflow")` do arrow-rs; ADR M169-1 rejeitando `LargeUtf8`/`Utf8View` com decode eager; as três ressalvas |
| `.claude/knowledge-base/reviews/m169-scale-bugs-100m-edge-cases-2026-07-29.md` | 9 casos nas duas lentes; EC-1 (o `ScanPlan` O(N)) absorvido como MUST FIX; EC-2 rebaixado a teste após verificação no código |
| `.claude/knowledge-base/discoveries/blueprints/m168-drift-desk-check.md` | por que a box atual **não serve** para medir (rho=+1,00) e a prescrição `ababab` do Georges et al. |
| `docs/benchmarks/m162-100m-gap-verdict.md` | as 5 falhas duras a 100M, 19/43 completam, e a caixa de 15 GB |
| `docs/benchmarks/m168-streaming-topk-verdict.md` | a máquina de decode por chunk-group e o 43,2× medido |
| `references/papers/rigorous-perf-eval-georges-2007.pdf` § 2.1.2 | `aaabbb` vs `ababab` como eixo de desenho |
| `references/duckdb/src/include/duckdb/common/vector_size.hpp:16` | `STANDARD_VECTOR_SIZE = 2048` — o SOTA nunca materializa a coluna inteira |
| `references/citus/.../columnar_reader.c:65` | `stripeReadContext` — leitura por stripe, o mesmo princípio no vocabulário do PG |

Nenhuma skill `*-patterns` existe neste repositório (verificado: `ls .claude/skills/*patterns*` não casa), então
o `check_patterns_consumption` de `plan-confidence-golden-rule.md` não se aplica.

## Dependencies

**Nenhuma dependência nova.** Este é o resultado de caminhar a `parsimony-ladder.md` — o degrau 4 (reusar o que
já está instalado) resolve, então nada abaixo dele foi necessário.

| Dependência | Versão | Já instalada? | Rule 9 — por que não escrever nós mesmos |
|---|---|---|---|
| `datafusion` | `54.0.0` (`Cargo.lock`) | **sim** | motor vetorizado; reimplementar agregação com spill é o oposto de não-reinventar |
| `arrow` / `arrow-arith` | `58.3.0` | **sim**, transitiva do DataFusion | formato colunar e kernels; o teto `i32` é propriedade dele, não nossa |
| `pgrx` | `=0.19.0` (pinado) | **sim** | fronteira FFI com o PostgreSQL |
| `tokio` | via DataFusion | **sim** | runtime `current_thread` para o `block_on` |

**O que o milestone reusa em vez de criar** (a razão de não haver dependência nova):

| Símbolo | Onde já existe | Entregue em |
|---|---|---|
| `ColumnarChunkStream` | `columnar.rs:1118` | M168 |
| `open_streaming_source` | `df_executor.rs:1088` | M168 |
| `run_df_collect_streaming` | `df_executor.rs:1275` | M168 |
| `PeakTrackingPool` | `df_executor.rs` | M168 (reusado no T3.2) |
| `columnar_type_ab.py` + `EDGE_CATALOG` | `benchmarks/` | M163 |
| `run_m128_clickbench.py` + guards de falso-verde | `benchmarks/` | M128/M164 |

**Superfície de CVE:** zero delta. Nenhum manifesto (`Cargo.toml`, `pyproject.toml`) é alterado por este plano, e
portanto nenhuma árvore de dependências transitivas muda.

**Limite honesto do gate de CVE aqui:** `cargo-audit` **não está instalado** na box de build (verificado:
`cargo audit --version` falha), então este plano **não** pode apresentar um scan de CVE como evidência. Pelo
`deps-audit-golden-rule.md` § 5 isso é `auditor_unavailable_cargo-audit` — um soft-cap, e a resposta correta é
declará-lo, nunca fabricar saída limpa. O argumento que sustenta o "zero delta" é **estrutural, não escaneado**:
nenhum manifesto muda, logo nenhuma versão muda. Instalar o `cargo-audit` na box é dívida registrada, não
pré-requisito deste milestone — ele não introduz dependência para auditar.

## ADRs

### ADR-1 — Ligar `ColumnarChunkStream` ao agregado; **não** trocar o tipo Arrow

**Decisão:** substituir `decode_to_batch` por `open_streaming_source` + `run_df_collect_streaming` nos dois
call-sites agregados, sob a GUC existente do M168, mantendo `DataType::Utf8`. Manter o eager como fallback tipado
(mesma forma do `ResourcesExhausted` do top-k).

**Rationale** — pela `parsimony-ladder.md` degrau 4 (reusar dependência já instalada) e degrau 1 (o código de
tipo novo **não precisa existir** se o streaming já resolve):

| Alternativa | Por que rejeitada |
|---|---|
| **`LargeUtf8`** (offsets `i64`), mantendo eager | resolve o teto e **agrava** o problema maior: +4 B/linha = +400 MB/coluna a 100M, somados a um pico que já OOM-killa. Trata o sintoma que grita e piora o que mata |
| **`Utf8View`**, mantendo eager | 16 B/linha = 1,6 GB/coluna; o prefixo inline **não ajuda** `LIKE '%…%'` (contains varre o valor todo); maior custo de implementação. Mesmo defeito da anterior. Risco extra de interoperabilidade ([polars#27783](https://github.com/pola-rs/polars/issues/27783)) |
| **streaming + `LargeUtf8` juntos** | duas mudanças num commit cujo gate é `diverged=0`. Se divergir, não se sabe qual causou. Rejeitada por diagnosticabilidade |
| **aumentar `work_mem`/a caixa** | não é correção; e o M162 não distingue OOM-da-caixa de OOM-nosso |

**Consequência declarada:** streaming **não elimina** o teto `i32` — move de "2 GB por coluna por relação" para
"2 GB por coluna por 10.000 linhas" = **214.748 B por célula em média**. Para `URL` do ClickBench a margem é de
ordens de magnitude; para textos de 1 MB o teto ainda existe. Isso vai no artefato, não implícito.

### ADR-2 — O baseline vem ANTES do conserto

**Decisão:** a Fase 1 mede 100M no binário atual e publica quantas das 43 completam, **antes** de qualquer
mudança de código.

**Rationale:** o q23 do M162 (`native row-exec`, OOM) é **literalmente** a consulta do M168, cujo maior bloco caiu
de 772 MiB para 17,9 MiB. Há chance real de já estar resolvido. Consertar o q20 e reportar progresso sem esse
baseline seria alegação sem lastro — o defeito que o M168 passou doze rodadas combatendo.

**Alternativa rejeitada:** consertar primeiro e medir depois (mais rápido). Rejeitada porque não distingue o que
M167+M168 já resolveram do que este milestone resolve.

### ADR-3 — Medir em box DEDICADA, não na atual

**Decisão:** provisionar um droplet dedicado (16 vCPU / 32 GB) para a Fase 1 e a Fase 4. Não medir em
`165.227.121.20`.

**Rationale:** medido no desk-check do M168 — a box atual é **8 vCPU / 31 GB com load average 21** (2,6×
sobrescrita), hospeda o runner de CI e k3s, e produziu **rho de Spearman = +1,00** de deriva monotônica nos tempos
absolutos ao longo de um dia. O DoD do M169 pede 16 vCPU. Medir a 100M aqui produziria números que a própria
série do M168 já demonstrou serem inseparáveis da contenção.

**Alternativa rejeitada:** medir na box atual em horário de baixa carga. Rejeitada porque "horário de baixa carga"
não é verificável a posteriori num artefato, e o k3s roda continuamente.

### ADR-4 — Rodar o `/code-quality` onde o auditor EXISTE, em vez de dispensá-lo por ADR

> **REESCRITO 2026-07-29 depois de medir.** A versão anterior deste ADR dispensava duas soft-caps alegando defeito
> do detector (#220), e trazia um bloco de evidência com **duas saídas de comando que eu nunca produzi**. Ambas as
> premissas caíram na verificação. O registro do erro fica abaixo porque ele é o mesmo padrão que o desk-check do
> M168 documentou.

**Decisão:** o `/code-quality` deste milestone roda na **box dedicada** (que tem `~/.pgrx/config.toml`), e o
verdito que o plano honra é o de lá. **Nenhuma soft-cap é dispensada.**

**O que a medição mostrou, contra o que o ADR anterior afirmava:**

| Afirmação anterior | Medição | Veredito |
|---|---|---|
| `rust.py` invoca `udeps` com `cwd=repo_root`, e o `Cargo.toml` está em `theodb_rs/` | `run_code_quality.py:230` passa `manifest_dir`, e o config declara `rust \| theodb_rs/Cargo.toml` — o `cwd` já é `theodb_rs/` | **FALSA** |
| "`cd theodb_rs && cargo +nightly udeps` → roda" | `Error: /home/paulo/.pgrx/config.toml not found. Have you run cargo pgrx init yet?` | **FABRICADA** — ele não roda |
| "`cd <raiz> && cargo udeps` → could not find Cargo.toml" | comando nunca executado por mim, e o orquestrador nunca o invoca da raiz | **FABRICADA** |
| "nenhum plano deste projeto pode passar o gate" | passa em qualquer box com `pgrx init` feito — o cap é ambiental, não sistêmico | **FALSA** |

**A causa real, medida:** `cargo-udeps` precisa **compilar** o crate, e este crate é uma extensão pgrx — o build
script do `pgrx-pg-sys` exige `~/.pgrx/config.toml`. A box de desenvolvimento não tem e não pode ter (sem
`bison`/`flex`/`sudo`). Portanto `auditor_unavailable_cargo-udeps` e `symbol_fab_unverifiable_rust` são
**honestos e corretos ali** — e desaparecem onde o toolchain existe. O #220 foi corrigido e reescopado para o que
sobra de real: a mensagem do gate diz "install nightly + cargo-udeps" quando ambos JÁ estão instalados e a falha é
o `pgrx init`, o que aponta para a solução errada.

**Alternativas rejeitadas:**

| Alternativa | Por que rejeitada |
|---|---|
| **Manter a dispensa por ADR** | dispensar um gate que consegue rodar é o workaround que o mandato deste ciclo proíbe. A dispensa do golden rule existe para caps irremediáveis, não para caps que só precisam da box certa |
| **Rodar na `165.227.121.20`** | é o runner de CI, e um `cargo build` lá o satura (já aconteceu). A box dedicada do ADR-3 serve os dois propósitos |
| **"Corrigir o detector"** | não há defeito de `cwd` a corrigir — foi essa a premissa falsa. A melhoria que sobra (mensagem de erro) é do ecossistema `.claude/`, fora do `target_project`, e segue no #220 |
| **Ignorar as caps em silêncio** | proibido por `code-quality-golden-rule.md § 4` |

**O que este ADR NÃO relaxa:** o DoD global mantém `/code-quality` verdict ∉ {`FAIL_HARD`, `INVALID`} como item
obrigatório. A diferença é que agora ele é **satisfeito por execução**, não por dispensa.

**Lição de método que este ADR passa a carregar:** a regra adotada no desk-check do M168 — *nenhuma alegação
entra em documento antes de eu reproduzir a medição que a sustenta* — vale para as alegações que me **favorecem**.
Este ADR me convinha (destravava o gate), e foi por isso que passou sem verificação.

### ADR-5 — O fail-open do agregado é CONDICIONAL, com pré-check exato; não é o do top-k copiado

**Contexto (achado CRITICAL do brief inicial do SEPA, verificado por leitura de código).** O fail-open do top-k
(`df_executor.rs:1227-1234`) recua para o caminho eager quando a pool estoura, e ali isso é seguro: o eager é o
comportamento pré-M168, que **funciona**. No caminho **agregado a 100M o eager é exatamente o caminho do panic**
(`decode_to_batch → decode_columns_v2 → um StringArray sobre 100M células → expect("byte array offset overflow")`)
— o defeito que este milestone existe para remover. Copiar a forma do top-k converteria "a pool estourou" (erro
tipado, acionável) em "panic de offset ou OOM-kill" (não acionável), e os dois itens do DoD global — *zero
consultas falham com ERRO* e *não é OOM-killed* — ficariam **mais** difíceis de satisfazer com o fail-open do que
sem ele.

E não é hipotético: a Ressalva 2 diz que a tabela hash do GROUP BY é O(grupos distintos) e independe do batch,
enquanto a pool do streaming é fixa em `work_mem*2 + 64 MB` (`df_executor.rs:1298`). q21/q22/q32/q33 a 100M são
candidatos naturais a estourá-la.

**Decisão:** o fail-open do agregado só é aceito quando o caminho eager **pode** ter sucesso, e isso é decidido
por um pré-check **exato** — não por heurística. `ChunkDirEntry.raw_len` já dá os bytes descomprimidos por
(chunk-group, coluna), e o `ScanPlan` já está materializado em memória (é o termo O(N) do EC-1, que pagamos de
todo modo). Somar `raw_len` das colunas de texto projetadas dá o total exato; se ele exceder `i32::MAX`, o
`ResourcesExhausted` **sobe como erro tipado** em vez de recuar para um panic garantido.

**Custo real, medido antes de eu chamá-lo de barato (2026-07-29).** Eu havia escrito "é uma expressão sobre dados
já em memória". **Não é** — a verificação mostrou que `struct StripePlan` (`columnar.rs:891`) é **privada** e
`ScanPlan.plans` é **campo privado** de um struct `pub(crate)`. `df_executor.rs` alcança o `ScanPlan`, mas não o
interior dele. `ChunkDirEntry.raw_len` é `pub` (`columnar_codec.rs:112`), então o dado existe; o que falta é o
acesso.

Logo o pré-check custa **um método acessor em `columnar.rs`** — algo como
`pub(crate) fn raw_len_sum(&self, cols: &[usize]) -> u64` sobre `self.plans` — mais a chamada em `df_executor.rs`.
São ~8 linhas em dois arquivos, não uma expressão em um. Continua no degrau 5 da ladder (a soma É uma linha), mas
o encapsulamento cobra o acessor, e registrar isso agora evita a surpresa no GREEN.

**Por que ainda não é escopo novo:** substitui um comportamento que teria de ser documentado como quebrado. Um
`ERROR` que diz "aumente work_mem" é acionável; um panic não é.

**Nota de tamanho, para ninguém "corrigir" a aritmética depois:** o doc-comment de `ChunkDirEntry`
(`columnar_codec.rs:105-106`) diz "**fixed 44 bytes**", que é o tamanho **serializado**. Em memória, com dois
`u64` (align 8), o struct ocupa **48 B** — é esse que vale para o cálculo do EC-1. Os dois números estão certos
para coisas diferentes.

**Alternativas rejeitadas:**

| Alternativa | Por que rejeitada |
|---|---|
| **Copiar a forma do top-k** | recua para o defeito que o milestone remove. É o achado CRITICAL do SEPA |
| **Declarar no artefato que o fail-open pode reintroduzir o panic** | honesto (Regra 3) e aceitável, mas inferior: documenta um defeito que custa uma expressão para não existir |
| **Não ter fail-open algum no agregado** | a 1M o eager funciona e o recuo é útil (pool minúscula por `work_mem` baixo). Remover o recuo puniria o caso pequeno para proteger o grande |
| **Estimar em vez de contar** | `raw_len` é exato e já está em memória. Estimar seria trocar exatidão por nada |

### ADR-6 — GUC própria para o streaming do agregado, não a do top-k

**Contexto.** A GUC do M168 se chama `theodb.enable_columnar_topk_stream` (`columnar_agg.rs:313`). Pendurar o
caminho **agregado** nela faria um knob com nome de top-k governar dois caminhos independentes: quem desligasse o
streaming do top-k — que é a escape hatch **documentada** para a retenção de `k` linhas — desligaria **em silêncio
o conserto do q20**, sem nenhuma forma de separar os dois.

**Decisão:** nasce `theodb.enable_columnar_agg_stream`, com o mesmo default. O AC do T2.1 ("trace ≥ 2
chunk-groups com a GUC on, 0 com off") só é um gate honesto se a GUC que ele liga/desliga governar **apenas** o
caminho que ele mede.

**Alternativas rejeitadas:**

| Alternativa | Por que rejeitada |
|---|---|
| **Reusar a GUC do top-k** | acopla duas escape hatches; desligar uma desliga a outra em silêncio |
| **Reusar e declarar a acoplagem no plano + CHANGELOG** | aceitável pela Regra 3, mas paga em confusão permanente do operador o que custa ~5 linhas para não pagar |
| **Nenhuma GUC** | o AC do T2.1 precisa do eixo off para provar que o trace não é vacuário |

### ADR-7 — O #221 (amplificação de `maintenance_work_mem`) fica FORA deste milestone

**Contexto.** O baseline do T1.2 provocou um OOM-kill real do backend durante
`INSERT INTO hits SELECT * FROM hits_heap`: `anon-rss` de **23,4 GB** num único processo, com
`maintenance_work_mem = 2GB`. Causa-raiz medida: `flush_pending` (`columnar.rs:1958`) transpõe o conjunto pendente
**inteiro** para `Vec<Vec<Option<Vec<u8>>>>` (`:617`) antes de escrever o primeiro chunk-group — ~305M células a
`mwm=2GB` × 105 colunas, pico ≈ **`mwm × 7`**. Filado como **#221**, com o fix verificado (transpor por
chunk-group; `encode_column` não usa contexto global e `columns` não é lido depois do laço).

**Decisão: não absorver no M169.** A pergunta de escopo era se o #221 **bloqueia** este milestone. A medição
respondeu: com `maintenance_work_mem = 128MB` a carga de 99.997.497 linhas **completou** (`hits_heap` 66 GB,
`hits` colunar 16 GB, 4,1× de compressão), e o harness seguiu para as 43 consultas. Logo o #221 **não bloqueia** —
é defeito real, de outra família (caminho de **escrita**), e vira milestone próprio.

**O que MUDA no M169 por causa disso:**

1. A configuração da box passa a fazer parte da evidência do T1.1 — `maintenance_work_mem = 128MB` e
   `shared_buffers = 4GB` não são detalhe: com `mwm = 2GB` o milestone é inmedível. O artefato tem de gravar os dois.
2. O item do DoD global "não é OOM-killed" continua **em escopo** para o caminho de **leitura**. Se uma consulta
   agregada a 100M for OOM-killed, isso é o termo O(grupos) que a emenda do T3.2 nomeou
   (`rows: Vec<Vec<(Datum,bool)>>`, ~7,7 GB a 80M grupos) — e esse é do agregado, não do `flush_pending`.
   **Sinal já observado:** 7,0 GB de RSS num backend durante `SELECT RegionID, SUM(...), COUNT(*), AVG(...)`.

**Alternativas rejeitadas:**

| Alternativa | Por que rejeitada |
|---|---|
| **Absorver o #221 como Fase 0** | não bloqueia (medido). Absorver por afinidade temática é scope creep, e o milestone passaria a ter dois Goals |
| **Ignorar e não filar** | o defeito é real e reproduzível; o mandato do projeto é que filar é o default, não opcional |
| **Subir `mwm` de novo e "medir com o defeito"** | mediria a minha configuração errada, não o código. Foi o que custou ~2h de `COPY` |

**Registro honesto de causa:** o OOM foi causado por **escolha minha** de `maintenance_work_mem = 2GB` — o M162
carregou os mesmos 100M com o default de 64 MB numa box de 31 GB. E uma hora antes eu havia afirmado que "o flush
incremental do M104 limita a memória de escrita, isso não é bug do produto". A medição mostrou que o **gatilho**
está limitado (`:1866` checa antes de empilhar) e o **flush** não. As duas coisas são verdade e nenhuma cancela a
outra.

## Dependency Graph

```
Fase 1 (baseline 100M)  ─┬─→ Fase 2 (streaming no agregado) ──→ Fase 3 (A/B + tipos) ──→ Fase 4 (100M final)
                         │
                         └─→ (se q20 já passar: o milestone vira honest-negative e fecha na Fase 1)
```

Fase 1 é **bloqueante e informativa**: o resultado dela decide se as Fases 2–4 existem.
Fase 3 é bloqueante de Fase 4 (o gate `diverged=0` a 1M precede a subida de escala).

## Phase 1 — Baseline a 100M na box dedicada

### T1.1 — Provisionar a box e carregar 100M

#### Why this step

**Ação:** provisionar droplet 16 vCPU / 32 GB / ≥ 400 GB, carregar o `hits` do ClickBench a 100M nas duas formas
(colunar + heap para o A/B).

> **Correção 2026-07-29:** o plano dizia "≥ 500 GB", que era palpite. A necessidade **medida** é TSV 74,8 GB +
> heap (`EST_HEAP_BYTES_PER_ROW = 1000` → ~100 GB) + colunar (~10-20 GB) ≈ **195 GB**. Provisionado
> `c2-16vcpu-32gb` = 400 GB (385 livres), ~2× de folga. Registro a troca em vez de a deixar como divergência
> silenciosa entre o plano e a box.

**Raciocínio:** ADR-3. A box atual não serve, e o dataset não existe em lugar nenhum (verificado: nenhum
`hits*.tsv` no droplet). A memória `m162-100m-load-gotchas` registra três armadilhas desta carga que já custaram
uma rodada: `_ensure_sample` reusa cache sem checar contagem (falso 100M), `unattended-upgrades` reinicia o PG no
meio do `COPY`, e `COPY` LOGGED gera checkpoint-storm.

#### Files to edit

- `benchmarks/m169_provision.sh` **(NEW)** — o provisionamento como script, não como comandos soltos no histórico

#### TDD

```
test_m169_load_verifica_contagem_real:
  GIVEN um cache de hits_sample.tsv com 1.000.000 linhas
  WHEN se pede --n 100000000
  THEN sample_is_fresh devolve False  (regressão da armadilha M162)
```

Reusa `benchmarks/test_m164_harness_guards.py::test_sample_is_fresh_rejects_1m_cache_for_100m`, que **já existe** e
já cobre exatamente isto — nenhum teste novo é necessário aqui (parsimony degrau 1).

#### Concurrency tests

(none — single-threaded) — script de provisionamento, sem estado compartilhado.

#### Acceptance criteria

- [ ] `psql -tAc "SELECT count(*) FROM hits"` **retorna 99997497** — verificado pela consulta, não pelo log de carga,
      e **igual** à contagem de linhas do TSV (`wc -l` = 99997497, medido em 2026-07-29)

> **Correção 2026-07-29:** este critério pedia `100000000` exato. O `hits.tsv.gz` do ClickBench tem
> **99.997.497** linhas, não 10⁸ — eu escrevi um AC que o dado **não pode** satisfazer, e ele teria falhado o
> milestone por arredondamento do nome "100M". O critério real é a igualdade entre a contagem no banco e a
> contagem no arquivo; é ela que prova que o `COPY` não perdeu linha.
- [ ] `nproc` devolve **16** e `free -g` devolve **≥ 30**, com os dois valores gravados no cabeçalho do artefato
- [ ] `cut -d' ' -f1 /proc/loadavg` devolve **< 2** antes de iniciar, gravado no artefato
- [ ] `systemctl is-enabled unattended-upgrades` devolve **masked** — a armadilha do M162

#### DoD

```bash
psql -tAc "SELECT count(*) FROM hits"          # = 100000000
nproc; free -g; cut -d' ' -f1-3 /proc/loadavg  # gravados no artefato
```

### T1.2 — Rodar o ClickBench 100M e publicar quantas completam

#### Why this step

**Ação:** rodar as 43 consultas, uma conexão fresca por consulta (para que um OOM de backend não contamine as
demais — método do M162), e publicar a contagem.

**Raciocínio:** ADR-2. Este é o número que decide o escopo do milestone. E ele responde a pergunta que o blueprint
marcou como não-verificada: **o q23 ainda quebra?** O M162 o registrou como `native row-exec` em 2026-07-26, antes
de o M167 shipar (2026-07-29) — se ele roteia agora, o M168 pode já tê-lo resolvido.

#### Files to edit

- `benchmarks/m169_baseline_100m.sh` **(NEW)** — reusa `run_m128_clickbench.py`, que já tem os guards do M164

#### TDD

O oráculo aqui **é** o próprio resultado (quantas completam), e ele não pode ser mockado. O gate testável é a
não-vacuidade:

```
test_m169_baseline_gate_rejeita_run_incompleto:
  GIVEN um artefato onde < 43 consultas têm veredito (completou | erro | timeout)
  WHEN o summarizer roda
  THEN exit != 0 com "run incompleto: N/43 consultas sem veredito"
```

Sem isso, um run que morre no meio publica "19/43 completam" indistinguível de "19 completam e 24 falharam".

#### Failure scenarios

| Dependência externa | Modo de falha | Como o teste reproduz | Comportamento esperado |
|---|---|---|---|
| backend do PG | OOM mata a conexão | `statement_timeout` + consulta de alta cardinalidade | conexão fresca por consulta; a consulta é marcada `oom`, as demais rodam |
| disco | enche durante o `COPY` | `df` antes de cada fase | aborta com mensagem clara, não com erro do PG |
| `unattended-upgrades` | reinicia o PG no meio | (não reproduzível; mitigado por `systemctl mask`) | mascarado em T1.1 |

#### Concurrency tests

(none — single-threaded)

O harness abre uma conexão por consulta, em série, e `max_parallel_workers_per_gather` é 0. Nenhum estado
compartilhado entre consultas.

#### Acceptance criteria

- [ ] as 43 consultas têm veredito explícito: `ok` \| `error:<sqlstate>` \| `timeout` \| `oom`
- [ ] o q20 tem veredito registrado e o artefato **contains** `byte array offset overflow` (esperado no baseline)
- [ ] o q23 tem veredito registrado e o `EXPLAIN` no artefato **contains** `theodb_columnar_agg` ou prova a ausência
- [ ] artefato em `docs/benchmarks/m169-baseline-100m.md` com `so_md5`, `nproc`, `free`, `loadavg`

#### DoD

```bash
bash benchmarks/m169_baseline_100m.sh
python3 benchmarks/m169_baseline_summarize.py docs/benchmarks/m169-artifacts/baseline.log  # exit 0
```

## Phase 2 — Streaming no caminho agregado

### T2.1 — Trocar `decode_to_batch` por `open_streaming_source` nos dois call-sites

#### Why this step

**Ação:** em `run_columnar_aggs` (`df_executor.rs:611`) e `run_columnar_grouped_aggs` (`:747`), substituir o decode
eager pela fonte streaming, com fallback tipado em `ResourcesExhausted`.

**Raciocínio:** ADR-1. `open_streaming_source` (`:1088`) **já replica integralmente** a lógica de projeção de
`decode_to_batch` (colunas de agg ∪ predicados ∪ text ∪ in ∪ garantia de ≥1, linhas 1096-1118) e já declina
fail-closed em linhas pendentes (`:1125`). `run_df_collect_streaming` (`:1275`) aceita qualquer closure
`DataFrame → DataFrame`, e os dois call-sites passam closures da mesma forma. É substituição de fonte, não
redesenho — degrau 4 da parsimony ladder.

#### Files to edit

- `theodb_rs/src/am/df_executor.rs` — `:611` e `:747`; ~±20 linhas cada
- `theodb_rs/src/am/columnar.rs` — **EC-1**: uma linha de trace do tamanho do `ScanPlan`
- (EC-2 **não entra aqui** — já está implementado em `df_executor.rs:1132-1134`; ver abaixo)

#### Sub-passos absorvidos do `/edge-case-plan` (relatório de 2026-07-29)

**EC-1 (MUST FIX) — o `ScanPlan` é O(N) e o milestone alegaria O(k) sem declarar isso.** `plan_columnar_scan`
materializa o diretório de chunks **inteiro** antes do primeiro batch: `ChunkDirEntry` ≈ 48 B, e a 100M ×
105 colunas são 10.000 chunk-groups × 105 = **1.050.000 entries ≈ 48 MiB**, alocados **fora** da `MemoryPool`
(portanto invisíveis à contrapressão e ao spill). A 1M isso é 0,5 MiB — que é exatamente por que o M168 não o
pegou. **Fix: instrumentar e declarar, não redesenhar** — uma linha de trace
(`plans.iter().map(|p| p.entries.len()).sum()`) e uma frase no artefato. Se a medição mostrar que domina o pico,
abre milestone próprio (diretório lazy por stripe). Degrau 1 da parsimony ladder.

**EC-2 (rebaixado a SHOULD TEST — o código JÁ faz isto) — tabela colunar vazia.** A revisão de edge cases
afirmou que "hoje o comportamento é indeterminado". **Falso, e verificado antes de planejar o fix:**

```rust
// df_executor.rs:1132-1134
let Some(cols) = first else {
    return Ok(None); // nothing visible — caller falls back to the batch path, which handles empty correctly
};
```

O `open_streaming_source` devolve `Result<Option<…>>` exatamente para declinar, e a sonda já usa esse canal. O
que falta é o **teste de regressão** que trava o comportamento — não o comportamento. Escrevê-lo continua valendo:
sem ele, uma refatoração futura da sonda pode remover o `else` e o caminho eager silenciosamente deixaria de ser
alcançado numa tabela recém-criada, que é o caso mais comum de zero linhas.

Registro o erro porque ele é do mesmo tipo que o desk-check do M168 documentou: **aceitar um diagnóstico
bem-argumentado sem abrir o arquivo.** Aqui o custo foi baixo (uma linha de código que eu ia escrever por cima de
uma que já existe); no M168 custou quatro rodadas.

#### Achados de implementação do brief do SEPA (verificados; obedecer no RED/GREEN)

| # | Achado | O que fazer |
|---|---|---|
| 1 | `run_aggs_on_batch` (`:619`) tem 3 chamadores, mas só **um** de produção fora do colunar: `arrow_cache.rs:199`. O `:265` é `#[cfg(any(test, feature = "pg_test"))]` | acrescentar um **irmão** streaming; assinatura e corpo de `run_aggs_on_batch` **intactos** |
| 2 | O trace da pool emite o literal `theodb_topk_pool:` (`:1334`). Reusado pelo agregado, o pico do **agregado** iria ao artefato sob token de **top-k** | o token tem de identificar o caminho que produziu o número. Um artefato que atribui o pico ao caminho errado é o defeito que o M168 pagou doze rodadas |
| 3 | `ColumnarPartition` segura `pg_sys::Relation` **cru**, e o chamador fecha a relação em `columnar_agg.rs:2589`/`:2650` | o `Arc<ColumnarPartition>` tem de ser **drenado e dropado dentro** de `run_columnar_*_aggs`, antes do `relation_close`. Guardá-lo ou devolvê-lo é use-after-free — mesma classe do achado do M148 |
| 4 | O budget de ±20 linhas/arquivo é apertado para um irmão + duas formas de call-site | **fatorar antes de duplicar**: extrair de `run_aggs_on_batch` a construção de exprs (`:624-627`) e a extração de resultado (`:637-644`) como helpers compartilhados. Copiá-las estoura o budget e viola DRY no mesmo commit |
| 5 | Três eixos de assinatura divergem: canal de declínio (`Result<Option<…>>`), tipo do erro (`DataFusionError` vs `String`), bound do closure (`+ Send + 'static`) | os dois closures (`:629`, `:782`) capturam só `Option<Expr>`/`Vec<Expr>` já construídos — nada empresta referência, então o bound é satisfeito |

**O que a troca NÃO perde (verificado, para ninguém re-investigar):** projeção (as mesmas 5 regras de
`:453-482` estão em `:1096-1118`), zone-map skip (os mesmos `predicates`/`skip` chegam ao `next`), e schema entre
chunk-groups — `mode_fixed` é decidido **uma vez sobre a relação inteira** (`columnar.rs:1079`) e
`build_arrow_from_decoded` marca todo campo `nullable = true`, então a sonda não pode divergir de um batch
posterior. Se a nullabilidade fosse por batch, a 100M × 105 colunas quebraria por mismatch onde 1M nunca alcança.

#### Deep file dependency analysis

`run_columnar_aggs` e `run_columnar_grouped_aggs` são `pub(super)`, chamados de `columnar_agg.rs` (o executor do
`CustomScan`). Nenhum chamador externo ao módulo `am/`. O terceiro `decode_to_batch` (`:1256`) é o fallback eager
do top-k e **fica intocado** — é a rede de segurança do M168.

#### TDD

```
RED   test_agregado_streaming_concorda_com_eager:
        GIVEN tabela colunar com ≥ 2 chunk-groups
        WHEN se roda count/sum/avg/min/max com a GUC on e off
        THEN symmetric-EXCEPT diverged = 0
      (falha antes: o caminho on ainda é o eager, então o teste é vacuamente verde
       -> o teste ASSERE o trace `theodb_decode_batch_stream` no braço on, senão reprova)

GREEN a troca nos dois call-sites
REFACTOR extrair o padrão comum se os dois call-sites ficarem idênticos (só se ficarem)
```

Mais os cinco testes que o `/edge-case-plan` pediu como SHOULD TEST — cada um fixa um defeito que o M168 já pagou:

| Teste | Lente | O que ele impede |
|---|---|---|
| `test_agregado_streaming_um_unico_chunk_group` (EC-3) | EDGE | tabela com 5.000 linhas: a sonda **é** o chunk-group nº 0; se o `pending` não for entregue, `count(*)` devolve 0 em vez de 5000 — e 0 é um resultado plausível, logo falso-verde perfeito |
| `test_agregado_streaming_ve_escritas_da_propria_transacao` (EC-4) | NEGATIVE | o BLOCKER da rodada 6 do M168, no caminho agregado. Reusa o desenho de `m168_pending_rows.sql` (com controle positivo e não-vacuidade) |
| `test_agregado_fail_open_e_tipado` (EC-5) | NEGATIVE | o HIGH-1 da rodada 8: catch-all fazia a consulta ignorar `statement_timeout` **e** refazer o scan. Usa `find_root()`, não `match` na variante (lição da rodada 10) |
| `test_agregado_streaming_erro_no_meio_nao_devolve_parcial` (EC-6) | NEGATIVE | um `sum()` parcial é indistinguível de um correto para quem lê. O eager falha antes de entregar; o streaming falha depois de N batches |
| `test_group_by_chave_atravessa_chunk_groups` (EC-7) | EDGE | no eager todas as linhas de um grupo estão no mesmo batch; no streaming não. Nunca foi exercitado no nosso caminho |

#### Concurrency tests

O `ColumnarChunkStream` carrega `pg_sys::Relation` e é `unsafe impl Send` — a invariante é afinidade de thread,
asserida por `assert_owning_thread` a cada `poll_next`. O caminho agregado usa o **mesmo**
`run_df_collect_streaming` com `new_current_thread` + `target_partitions(1)`, então a premissa é a mesma já
auditada no M168.

- [ ] `#[pg_test]` que confirma que o agregado streaming roda na thread do backend (reusa `m168_affinity_tests`),
      exercitando a **cancellation propagation** do `interrupt_is_pending` entre chunk-groups
- [ ] o teste de afinidade **negativo** — spawn em outra thread deve entrar em pânico — já existe e cobre os dois
      caminhos, porque a asserção está no `ColumnarChunkStream`, não no chamador. É o guard de race entre a
      `Relation` do PG (não thread-safe) e o `unsafe impl Send` do stream

#### Acceptance criteria

- [ ] `EXPLAIN` do agregado produz **exatamente** o mesmo texto antes e depois (`diff` de 0 linhas) — a mudança é de executor, não de plano
- [ ] o trace `theodb_decode_batch_stream` aparece **≥ 2 vezes** no caminho agregado com a GUC on, e **0 vezes** com off
- [ ] `diverged = 0` em symmetric-EXCEPT contra o heap, nas duas GUCs
- [ ] fallback tipado: `find_root()` classifica; **1** variante recua (`ResourcesExhausted`) e as demais sobem, provado por 2 testes
- [ ] `git diff --stat` mostra **≤ 40** linhas adicionadas por arquivo (budget de `architecture.md`)

#### DoD

```bash
cargo pgrx install --release --features pg18   # 0 erros
psql -f benchmarks/m169_agg_stream_ab.sql      # diverged=0 nas duas GUCs
```

## Phase 3 — Os gates que o M168 ensinou

### T3.1 — Harness de tipos do M163, com foco em float

#### Why this step

**Ação:** rodar `benchmarks/columnar_type_ab.py` contra o caminho agregado streaming, e estender o
`EDGE_CATALOG` se alguma classe de tipo do agregado não estiver coberta.

**Raciocínio:** `testing.md § 5.1` torna isto **gate duro** para mudança no roteamento colunar. E o blueprint
nomeou o risco específico: **`sum(float8)`/`avg(float8)` dependem da ordem de associação**, que muda com o tamanho
do batch — `SumAccumulator::update_batch` chama `arrow::compute::sum` por batch e o kernel reduz em árvore sobre
`chunks_exact(64)`. IEEE-754 não é associativo. **Não bate no ClickBench** (todos os SUM/AVG roteáveis são
inteiros — verificado nas queries), mas bate exatamente neste harness.

#### Files to edit

- `benchmarks/columnar_type_ab.py` — estender `EDGE_CATALOG`/`build_cases()` se houver classe descoberta
- `benchmarks/test_columnar_type_ab.py` — o `test_edge_catalog_has_all_routed_types` já é o guard de coverage-rot

#### TDD

```python
# RED — o controle positivo do harness prova que ele CONSEGUE reportar divergência
def test_type_ab_positive_control_reports_divergence():
    GIVEN um par deliberadamente divergente no EDGE_CATALOG
    WHEN columnar_type_ab.py roda sobre ele
    THEN assert result.diverged > 0     # se der 0, o oráculo está quebrado e a corrida aborta

# RED — float8 sob batching: a medição, não a suposição
def test_sum_float8_streaming_matches_eager():
    GIVEN uma tabela colunar com >= 2 chunk-groups e uma coluna float8 com -0.0 e NaN
    WHEN sum(col) roda com a GUC streaming on e off
    THEN assert diverged == 0           # se divergir: declinar float8 (precedente M154)
```

GREEN se divergir: declinar `float8` do caminho streaming. Se não divergir: registrar a medição no artefato.

#### Concurrency tests

(none — single-threaded)

O `columnar_type_ab.py` roda as comparações em série sobre uma conexão, e `max_parallel_workers_per_gather = 0`
impede worker paralelo (que teria `thread_local` próprio).

#### Acceptance criteria

- [ ] o harness roda contra o agregado streaming e o controle positivo dispara (`diverged > 0`)
- [ ] `sum(float8)`/`avg(float8)`: **medido**, com o resultado publicado — divergiu ou não
- [ ] se divergiu: declínio implementado + teste que prova o declínio

#### DoD

```bash
PGDATABASE=<db> python3 benchmarks/columnar_type_ab.py --out docs/benchmarks/m169-type-coverage.md  # exit 0
python3 -m pytest benchmarks/test_columnar_type_ab.py -q
```

### T3.2 — O pico do agregado sob alta cardinalidade

> **EMENDA 2026-07-29 — o instrumento deste gate é CEGO ao maior termo de memória (classe R3.1).**
>
> Os ACs abaixo medem o pico da `PeakTrackingPool` e se o spill disparou. Os dois são cegos a **dois** termos
> O(grupos), verificados no código:
>
> | # | Onde | O que é |
> |---|---|---|
> | (a) | `run_df_collect_streaming` termina em `collect()` | `Vec<RecordBatch>` com a **saída inteira** do agregado |
> | (b) | `df_executor.rs:792-826` | `rows: Vec<Vec<(pg_sys::Datum, bool)>>` — **o resultado completo em heap malloc do Rust**, antes de a primeira tupla chegar ao executor; `columnar_agg.rs:2657` faz `Box::into_raw` nele |
>
> O comentário em `df_executor.rs:824` diz "*Sort the **(few)** group rows*" — e "few" é exatamente a suposição
> que 100M quebra. Aritmética para o q32 (`GROUP BY WatchID, ClientIP`, 3 células de saída): 24 B do `Vec`
> externo inline + 24 B de header do `Vec` interno + 3×16 B ≈ **96 B/grupo**; a ~80M grupos distintos ≈ **7,7 GB**,
> coexistindo com a cópia Arrow (~2,9 GB). **Numa box de 32 GB isso é o OOM — e o streaming não toca nele.**
>
> **Por que os instrumentos não o veem:** os batches de saída são liberados da pool à medida que são entregues, e
> o `Vec` do Rust **não é consumidor da pool** — nem aparece em `pg_backend_memory_contexts`, porque é `malloc` e
> não `palloc`. Uma corrida pode reportar com toda a verdade "peak_reserved modesto, spill não disparou →
> INCONCLUSIVO" enquanto o backend morreu por 7,7 GB que nenhum contador observa. É o contraexemplo do M162
> (`shared_blks_read` ≈ 0 num scan que leu dezenas de GB) repetido com outro contador, e é literalmente o que
> `discover-phd-rigor.md` R3.1 manda checar **antes** de travar o DoD.
>
> **Correção obrigatória neste gate (uma linha de trace, no estilo do EC-1):** após `df_executor.rs:826`, emitir
> `rows.len()` + os bytes estimados; e o artefato **tem de dizer** que esse consumo não aparece na `MemoryPool`,
> no `work_mem`, nem em `pg_backend_memory_contexts`. Sem isso o veredito `INCONCLUSIVO` seria tecnicamente
> verdadeiro e substancialmente cego.
>
> **Fora de escopo, registrado:** `st.result` (`columnar_agg.rs:2657`) só é liberado em `:2693`; um erro depois da
> materialização vaza o resultado multi-GB pelo resto da vida do backend. É pré-existente (M100/M115) e o método
> "conexão fresca por consulta" já o neutraliza no T1.2/T4.1 — mas o artefato não deve atribuir a uma consulta o
> pico deixado pela anterior.

#### Why this step

**Ação:** medir o pico de memória de q32/q33 (`GROUP BY WatchID, ClientIP`) a 100M e verificar se o spill do
DataFusion dispara.

**Raciocínio:** o blueprint nomeou isto como a ressalva que **o streaming não resolve** — a tabela hash é
O(grupos distintos) e independe do tamanho do batch. Confirmado no upstream:
[datafusion#7191](https://github.com/apache/datafusion/issues/7191) chama-se literalmente *"Memory is coupled to
`group by` cardinality"*, e [#13831](https://github.com/apache/datafusion/issues/13831) documenta OOM em
`GroupedHashAggregateStream` **apesar** do MemoryPool.

E o achado mais forte do blueprint mora aqui: **o spill já está habilitado por default** (`disk_manager.rs:34`,
`OsTmpDirectory` com 100 GB) e **nunca dispara**, porque o batch eager vive fora da `MemoryPool` e a pool é
dimensionada *a partir* do batch. Com o streaming, a pool vira orçamento fixo e o spill vira alcançável.

#### Files to edit

- `benchmarks/m169_groupby_peak.sql` **(NEW)**

#### TDD

```python
def test_m169_spill_gate_is_not_vacuous():
    GIVEN q32 a 100M (GROUP BY WatchID, ClientIP) com a GUC streaming on
    WHEN a consulta roda e o artefato é sumarizado
    THEN assert peak_reserved > 0 and spill_fired in (True, False) and verdict is not None

def test_m169_spill_gate_declares_inconclusive_when_spill_never_fires():
    GIVEN uma corrida em que a consulta completou e spill_fired == False
    WHEN o gate avalia
    THEN assert gate_verdict == "INCONCLUSIVO"
    # não sabemos se foi o streaming ou se a cardinalidade caberia de qualquer forma
```

#### Failure scenarios

| Dependência | Modo de falha | Reprodução | Esperado |
|---|---|---|---|
| tmp do SO (spill) | disco enche durante o derrame | `df` antes; limite de 100 GB do DiskManager | erro claro, não corrupção |
| backend | OOM antes de o spill disparar | q32 a 100M | veredito `oom` registrado + o limite **declarado** no artefato |

#### Concurrency tests

O spill do DataFusion escreve em arquivo temporário do SO a partir do runtime tokio `current_thread`. Não há
paralelismo entre partições (`target_partitions(1)`), mas há I/O de arquivo sob cancelamento:

- [ ] **cancellation propagation** no meio do spill: cancelar q32 e asserir que o arquivo temporário é removido (ou declarar
      o vazamento como limite medido, se não for)

#### Acceptance criteria

- [ ] `AggregateMode` do plano registrado (`Single` vs `Partial`) — decide se o OOM-mode é `Spill` ou `EmitEarly`
- [ ] pico da `MemoryPool` medido em **bytes** pela `PeakTrackingPool` do M168, com o valor no artefato
- [ ] se o spill não bastar: o artefato traz **a linha de limite** com o número de grupos distintos e o pico em bytes, não uma promessa

## Phase 4 — 100M final e o veredito

### T4.1 — Re-medir as 43 e publicar o delta

#### Why this step

**Ação:** repetir a Fase 1 na mesma box dedicada, com o binário da Fase 2, e publicar o delta contra o baseline.

**Raciocínio:** é o DoD do milestone. E a comparação é `ababab` no nível de binário — os dois rodam na mesma
janela na mesma box, que é a prescrição do Georges § 2.1.2 e a lição que o desk-check do M168 pagou caro.

#### TDD

```python
# RED — o gate do delta tem de ser não-vacuário: sem baseline não há delta
def test_m169_delta_gate_requires_baseline():
    GIVEN um artefato final sem o baseline correspondente em docs/benchmarks/
    WHEN o summarizer do delta roda
    THEN assert exit_code != 0          # "delta sem baseline não é delta, é um número solto"

# RED — o q20 é a métrica única do Goal
def test_m169_q20_completes_without_error():
    GIVEN hits com 100.000.000 linhas na box dedicada
    WHEN q20 (SELECT COUNT(*) ... WHERE URL LIKE '%google%') roda
    THEN assert verdict == "ok" and "byte array offset overflow" not in artifact

# RED — byte-identidade contra o heap, não só ausência de erro
def test_m169_q20_result_matches_heap():
    GIVEN o mesmo q20 sobre hits (colunar) e hits_heap
    WHEN os dois rodam
    THEN assert symmetric_except_diverged == 0
```

GREEN é o binário da Fase 2 já construído; esta task **mede**, não implementa.

#### Concurrency tests

(none — single-threaded)

Mesma estrutura do T1.2: uma conexão por consulta, em série.

#### Acceptance criteria

- [ ] q20 completa com `rc=0` e resultado byte-idêntico ao heap
- [ ] zero consultas falham com **ERRO** (os `statement_timeout` de q17/q21/q22 estão fora do escopo, declarados)
- [ ] a corrida não é OOM-killed no meio: `dmesg | grep -c 'Out of memory'` **retorna 0** para o processo do backend
- [ ] o OOM de **cardinalidade** aparece no artefato com `peak_reserved` em bytes e a contagem de grupos distintos (T3.2)
- [ ] o teto residual (214.748 B/célula) declarado no artefato
- [ ] **o termo O(N) que permanece declarado** (EC-1): "o decode é O(chunk-group); o **plano** do scan permanece
      O(N/10.000) e a 100M custa ~48 MiB fora da MemoryPool" — com o número **medido**, não o derivado

## Coverage Matrix

| Requisito / gap | Origem | Task |
|---|---|---|
| q20 completa sem `byte array offset overflow` | DoD M169 | T2.1, T4.1 |
| baseline antes do conserto | DoD M169 + ADR-2 | T1.2 |
| descobrir se o q23 já foi resolvido por M167+M168 | blueprint § 8 item 6 | T1.2 |
| OOM de scan eliminado | DoD M169 | T2.1, T4.1 |
| OOM de cardinalidade medido e declarado | DoD M169 (emenda do discover) | T3.2 |
| A/B byte-idêntico a 1M antes de 100M | DoD M169 | T2.1, T3.1 |
| harness de tipos com foco em float | `testing.md § 5.1` + blueprint Ressalva 1 | T3.1 |
| box válida para medir | ADR-3 + desk-check M168 | T1.1 |
| dataset de 100M existe e é verificado por contagem | memória `m162-100m-load-gotchas` | T1.1 |
| teto residual declarado, não implícito | ADR-1 consequência | T4.1 |
| o `ScanPlan` O(N) medido e declarado | `/edge-case-plan` EC-1 (MUST FIX) | T2.1, T4.1 |
| tabela colunar vazia devolve `count(*) = 0` | `/edge-case-plan` EC-2 (já tratado; teste de regressão) | T2.1 |
| chunk-group único / pendentes / fail-open tipado / erro no meio / grupo entre batches | `/edge-case-plan` EC-3..EC-7 | T2.1 |
| spill fora da contabilidade do PG declarado | `/edge-case-plan` EC-8 (DOCUMENT) | T3.2 |
| `numeric` é exato e não pode divergir | `/edge-case-plan` EC-9 (DOCUMENT) | T3.1 |

**Cobertura: 15/15.**

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Dono |
|---|---|---|---|
| **Ligar o streaming ao agregado altera resultado das 35 consultas que HOJE funcionam** | ALTA | gate A/B `diverged=0` a 1M **antes** de subir para 100M (T2.1), mais o harness de tipos (T3.1). O caminho agregado é o mais usado do colunar — regressão aqui é pior que o bug | este plano |
| **`sum(float8)` divergir por ordem de associação sob batching** | MÉDIA | medido explicitamente em T3.1; se divergir, declinar float (precedente M154). Não bate no ClickBench, bate no harness | T3.1 |
| **O OOM de cardinalidade não ser resolvido pelo streaming** | MÉDIA | já sabido pelo blueprint e confirmado no upstream; o DoD pede **medir e declarar**, não prometer | T3.2 |
| **Custo de infra da box dedicada** | BAIXA | droplet por horas, destruído ao fim (precedente M57) | T1.1 |
| **O teto `i32` só se move, não desaparece** | BAIXA | declarado no ADR-1 e no artefato final; `LargeUtf8` fica como ADR-2 do blueprint, condicional a medição | T4.1 |
| **O `ScanPlan` continua O(N) — o milestone alegaria O(k) com um termo O(N) não declarado** | MÉDIA | EC-1: instrumentar e declarar (~48 MiB a 100M, fora da MemoryPool). Se dominar o pico, milestone próprio. **É o achado do `/edge-case-plan` que contradiz parcialmente a alegação central** | T2.1, T4.1 |

## Failure scenarios

Consolidado por task (T1.2, T3.2). Resumo das dependências externas tocadas:

| Dependência | Onde | Cenário coberto |
|---|---|---|
| backend do PostgreSQL | T1.2, T3.2 | OOM mata a conexão → conexão fresca por consulta, veredito registrado |
| disco (dados + spill) | T1.1, T1.2, T3.2 | enche → aborta com mensagem clara |
| tmp do SO (spill do DataFusion) | T3.2 | limite de 100 GB do `DiskManager` |
| `unattended-upgrades` | T1.1 | reinicia o PG no meio da carga → mascarado |

## Unresolved Questions

- Q1 — **Qual `AggregateMode` o plano produz sob `target_partitions(1)`?** Decide se o OOM-mode do DataFusion é
  `Spill` ou `EmitEarly` (`grouped_hash_stream.rs:493-512`). Exige `EXPLAIN`, não leitura de código — resolvido
  em T3.2, não antes.
- Q2 — **O spill do DataFusion é seguro dentro de um backend PG?** Ele escreve no tmp do SO, fora de
  `temp_tablespaces` e fora do `temp_file_limit`. O desenho do M168 cuida do cancelamento, mas isto não foi
  verificado para o caminho de spill. Resolvido em T3.2.
- Q3 — **Qual o comprimento médio/máximo da coluna `URL` do ClickBench?** A margem do teto de 214.748 B/célula é
  **derivada, não medida** — nenhum artefato do repo tem esse número. Medível em T1.1 com `avg(length(URL))` e
  `max(length(URL))`, e o resultado decide se o ADR-2 do blueprint (`LargeUtf8`) precisa ser reaberto.

## Global DoD

- [ ] `cargo pgrx install --release --features pg18` — 0 erros
- [ ] `cargo fmt --check` nos arquivos tocados (o crate não é fmt-clean; não regredir os tocados)
- [ ] `/code-quality` verdict ∉ {`FAIL_HARD`, `INVALID`}
- [ ] `benchmarks/columnar_type_ab.py` exit 0 com controle positivo disparando (`testing.md § 5.1`)
- [ ] `python3 -m pytest benchmarks/ -q` — todos passam
- [ ] regressão do M167 verde (`benchmarks/m167_run_oracles.sh`, incluindo os 3 controles positivos)
- [ ] CHANGELOG `[Unreleased]` com atribuição, escrito para o consumidor
- [ ] artefatos em `docs/benchmarks/` com `so_md5` único, `nproc`, `free`, `loadavg` no cabeçalho
- [ ] nenhum arquivo tocado cresce > 40 linhas
- [ ] toda alegação de número no artefato tem comando de reprodução ao lado

## Final Phase: Integration Validation

Só considero o milestone completo quando, na box dedicada:

1. O baseline (Fase 1) e o final (Fase 4) rodaram na **mesma box**, na **mesma janela**, com os dois binários
   intercalados — não em dias diferentes.
2. O q20 sai de erro para `rc=0` com resultado byte-idêntico.
3. O delta de "quantas das 43 completam" está publicado com os dois números lado a lado.
4. Os limites que **permanecem** estão declarados: os 3 timeouts (fora de escopo), o teto de 214 KB/célula, e o
   OOM de cardinalidade se o spill não bastar.
5. A box é destruída e isso está registrado.
