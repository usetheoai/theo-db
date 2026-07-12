# ADR-0039 — M89: ambuild streaming (build de memória limitada) — veredito MEDIDO + desvio parsimony do plano

- **Status:** Accepted (2026-07-12)
- **Contexto:** M89 (linhagem pós-v7 "build escalável a bilhão-scale") — fecha o teto de memória do `ambuild`
  descoberto no M88 (`docs/adr/0038`): o build do `theodb_ivfflat` picava ~4× o dataset base em RAM → 2 OOM-kills
  a 30M num box de 62 GB usáveis, capando o M88 a 16M. Objetivo do M89: 30M constrói num box de 64 GB com pico
  ≤ ~1.5× base, MEDIDO, sem mudar o formato on-disk e sem regressão.
- **Relação:** consome o achado do `docs/adr/0038` (o teto de ~4×). Artefato: `docs/benchmarks/m89-ambuild-streaming.{md,json}`.

## Decisão

O DoD do M89 é **`DOD_MET`** (MEDIDO, DO m-8vcpu-64gb, Xeon 8358, 62 GB usáveis, 30M×128 = 15.4 GB base):

1. **v5 f32: pico 19.7 GB = 1.28× base** (build 2128s, size 15 GB) — completa.
2. **v6 sq8: pico 23.1 GB = 1.50× base** (build 1990s, size 4.46 GB, SQ8 3.36× menor) — completa.
3. **old-build (pré-M89): 4.21×/64.7 GB → OOM** (reproduz o M88, medido neste run).
4. **Ambos completam a 30M num box 64 GB.** 250 pg_tests GREEN, zero regressão; formato on-disk inalterado
   (sem magic bump, sem REINDEX — os testes v5/v6 scan==seqscan provam que o streaming writer é byte-correto).

## Como (2 incrementos, ambos byte-idênticos ao formato)

1. **Clone-elimination:** `IvfflatIndex::build_owned` MOVE o corpus para o índice (sem clonar em `self.vectors`);
   AQ/SQ8 treinam do índice por referência; deleta o clone `corpus_vecs`.
2. **Streaming page-writes (a mudança-chave):** os writers v5/v6 recebem POSITIONS + `vectors`/`ids` por referência
   (elimina o clone `list_entries()`) e escrevem cada lista on-the-fly liberando o blob f32 por-lista (elimina o
   `enc_vec` + o buffer `items` que copiavam tudo antes do flush).

## Desvio do plano (Regra 3 / parsimony ladder rung 1)

O plano (`knowledge-base/plans/ambuild-streaming-plan.md`, plan-confidence SHIPPABLE_WITH_CAVEATS) e o grill
escolheram a **FFI do `tuplesort` do Postgres (Opção B)**. A **implementação NÃO usou FFI.** Justificativa MEDIDA:

- O Incremento 1 (clone-elimination) foi re-medido isolado e **ainda OOMou a 4.21×** — as cópias dominantes eram
  o clone `list_entries()` (16 GB) + o buffering `enc_vec`/`items` dos writers (~32 GB), não o clone do build.
- O Incremento 2 (streaming page-writes) **atinge o DoD de 30M (1.28×/1.50×) com risco muito menor (zero FFI).**
  A FFI do `tuplesort` era **YAGNI para o alvo de 30M** — a parsimony ladder (rung 1: "isto precisa existir?")
  resolve no streaming das escritas, não numa FFI C interna.

Isto é um desvio **parsimony-positivo justificado por medição** — solução mais simples, DoD atingido, menor risco.
Não é workaround: resolve genuinamente o OOM de 30M com memória bounded-por-lista, formato byte-idêntico, zero FFI.

## Consequências e limites honestos

- **NÃO é `O(maintenance_work_mem)`.** O pico ainda carrega a cópia 1× (`idx.vectors`, ~15.4 GB a 30M). Logo
  **100M (~51 GB base) ainda não cabe em RAM commodity** — o streaming verdadeiro via `tuplesort` (nunca
  materializar os vetores: heap→sorter→páginas) é o **follow-up honesto para 100M+**. M89 entrega o DoD de 30M,
  não o build bilhão-scale.
- v6/SQ8 fica no limite 1.50× (buffer `sq8_codes` ~3.8 GB); streaming do SQ8 por-lista baixaria mais (não
  necessário p/ o DoD).
- v3/v4 (interleaved, não storage-separated) mantêm o path antigo (legado/não-DoD; ainda OOMam a bilhão-scale).

## Alternativas consideradas

- **FFI do tuplesort agora (o plano)** — rejeitado p/ o DoD de 30M: risco alto (unsafe/virtual-slot) sem
  necessidade medida. Reservado como follow-up de 100M (onde a cópia 1× vira o gargalo).
- **Box maior (128 GB) p/ forçar 30M com o build antigo** — rejeitado: mascara a ineficiência acidental em vez de
  removê-la (Esforço ≠ Complexidade; atacar a causa essencial).

## Relação com ADRs anteriores

- Consome `0038` (M88, o teto de ~4× descoberto). Não altera `0002`/`0033`.
