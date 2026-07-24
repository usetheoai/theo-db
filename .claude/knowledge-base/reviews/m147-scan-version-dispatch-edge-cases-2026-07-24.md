# Edge Case Review — m147-scan-version-dispatch

Date: 2026-07-24
Tasks analyzed: 4 (T1.1, T2.1, T3.1, T4.1)
Cases found: 4 (EDGE: 2, NEGATIVE: 2 | MUST FIX: 3, SHOULD TEST: 1, DOCUMENT: 0)

## MUST FIX

### EC-1: o A/B (T4.1) não cobre o v3, mas o refactor de dispatch (T1.1) o afeta

- **Affected task:** T4.1 (e T1.1)
- **Kind:** EDGE (extremo válido — a versão de fallback)
- **Family:** Format / Boundary
- **Scenario:** O v3 (IVF f32, sem AQ) grava um discriminante explícito `3u32` em bytes [4..8] (`theodb_rs/src/am/page/ivf.rs:66`), MAS o if-ladder atual o trata como **else** (`theodb_rs/src/am/scan.rs:564` — cai no v3 qualquer coisa que não seja v4/v5/v6/v7/v8). O refactor da T1.1 substitui o if-ladder por `match ivf_version`, que muda como o v3 é alcançado. O A/B da T4.1 só constrói e testa v4/v5/v6/v7/v8 — **um regressão no caminho v3 passaria despercebida.**
- **Impact:** o dispatch de um índice v3 poderia regredir (top-k errado ou erro) sem o A/B acusar, violando o Goal "comportamento byte-idêntico".
- **Suggested fix:** T4.1 passa a construir e diffar **6 caminhos (v3, v4, v5, v6, v7, v8)**, não 5; a Coverage Matrix e o Goal referenciam "os 6 caminhos IVF-AQ + v3".

### EC-2: `ivf_version` deve preservar o comportamento do fallback do v3 (estrito vs permissivo)

- **Affected task:** T1.1
- **Kind:** NEGATIVE (versão inesperada — invalid input)
- **Family:** Format / Input
- **Scenario:** O if-ladder atual é **permissivo**: um índice com magic TIVS mas versão não-{4,5,6,7,8} cai no caminho v3 (o else, `scan.rs:564`). Se o novo `match ivf_version` for **estrito** (`3 => V3, _ => Err`), muda o comportamento para uma versão inesperada — de "tenta ler como v3" para "erro tipado". A T1.1 não declara qual política adota.
- **Impact:** mudança de comportamento silenciosa; um índice de versão desconhecida (ex.: gravado por uma versão futura) que hoje é lido como v3 passaria a falhar (ou vice-versa).
- **Suggested fix:** T1.1 declara a política explicitamente — **estrito** (`match { 3=>V3, 4=>V4, …, 8=>V8, other => Err("unknown IVF version {other}") }`), que é o correto (versão desconhecida = erro tipado 22023/XX002, não silêncio); e a acceptance criteria da T1.1 testa `ivf_version` sobre versão 99 → `Err`, e o A/B (T4.1) prova que v3 legítimo (versão==3) ainda lê idêntico.

### EC-3: `ivf_version` pode panicar (XX000) num bloco com magic mas < 8 bytes — regressão do M146

- **Affected task:** T1.1
- **Kind:** NEGATIVE (input truncado)
- **Family:** Format / Resource
- **Scenario:** A leitura do discriminante de versão faz `m[4..8].try_into().unwrap()` (o padrão de `ivf.rs:134`). Se o bloco 0 tem magic TIVS (passa o gate de `peek_magic`) mas foi truncado para < 8 bytes, o slice `[4..8]` panica no `unwrap()` → XX000 `internal_error` — exatamente a classe de regressão que o M146 acabou de eliminar (panic através de C).
- **Impact:** um índice corrompido/truncado deriva o backend com XX000 em vez do XX002 tipado que o M146 estabeleceu.
- **Suggested fix:** `ivf_version` checa `if m.len() < 8 { return Err("truncated IVF header") }` antes de ler [4..8]; a acceptance da T1.1 inclui esse caso; o `corrupt_index.sh` (já parametrizado por AM=theodb_ivfflat no M146) exercita o caminho.

## SHOULD TEST

### EC-4: o A/B (T4.1) precisa de um baseline CAPTURADO, não de dois binários vivos

- **Affected task:** T4.1
- **Kind:** EDGE (o mecanismo de comparação)
- **Suggested test:** `ab_scan_versions.sh` captura os resultados do baseline (ids + distâncias arredondadas por versão) num arquivo committado ANTES do refactor (build do commit `74fe445`), e o A/B compara o binário NOVO contra esse arquivo fixo — o padrão "corpus versionado" do lance (`test_data/` + assert de proveniência). Assertar: diff vazio nos 6 caminhos. Isto evita a ambiguidade de "ter dois binários ao mesmo tempo" e torna o A/B reproduzível.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 0 | 2 | 2 (EC-2, EC-3) | 0 | 0 |
| T2.1 | 0 | 0 | 0 | 0 | 0 |
| T3.1 | 0 | 0 | 0 | 0 | 0 |
| T4.1 | 2 | 0 | 1 (EC-1) | 1 (EC-4) | 0 |

**Coverage check:** T1.1 (o dispatch) tem os dois lados cobertos — EDGE (v3 fallback) e NEGATIVE (versão desconhecida + bloco truncado). T2.1 (Result+?) e T3.1 (kernel) não introduzem novos boundaries de input — operam sobre bytes já validados pelo decode; seus riscos (taxonomia, misparse) já estão em Drawbacks & Risks R1/R4 do plano. T4.1 é validação, seus edges são de método (EC-1, EC-4).

**Verdict:** PLAN NEEDS ADJUSTMENT (3 MUST FIX — todos refinamentos de T1.1/T4.1, sem expandir escopo nem adicionar módulo)
