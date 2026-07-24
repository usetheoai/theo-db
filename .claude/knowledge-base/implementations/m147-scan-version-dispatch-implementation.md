# Implementation — M147 scan version-dispatch

**Plan:** `.claude/knowledge-base/plans/m147-scan-version-dispatch-plan.md` (SHIPPABLE_WITH_CAVEATS 89)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m147-scan-version-dispatch-blueprint.md` (89)
**Baseline A/B:** `docs/benchmarks/m147-ab-baseline.txt` (30 linhas, 6 versões × 5 queries, binário pré-refactor 6e648fa)
**Substrato:** droplet PG 18.4 (pgrx 0.19); A/B via `ab_scan_versions.sh compare`.

## Descoberta que refina a Task 1.1 (durante a implementação, Regra 3)

O "caminho v3" do dispatch NÃO é só a versão 3 — `read_ivf_meta` (`page/ivf.rs:135`) aceita **ver==2 E ver==3**:
o v2 (M34, gen_base implícito) é lido pelo mesmo corpo, auto-migrado para v3 na primeira VACUUM fold. Logo o
`map_ivf_version` estrito deve mapear **`2 | 3 => V3`**, não só `3 => V3` — senão um índice v2 legado (hoje
lido via o else) passaria a dar `Err`, uma regressão. O `enum IvfVersion` representa os 6 CAMINHOS DE SCAN
(v3-path cobre disc. 2 e 3), não os 7 discriminantes on-disk. Comportamento de erro: uma versão desconhecida
(ex.: 99) passa de "unsupported structured format v99" (read_ivf_meta) para "unknown IVF version 99"
(map_ivf_version) — ambos Err tipados (classe equivalente), mensagem diferente; o A/B não testa mensagens de
erro de índices inválidos, só top-k de índices válidos, então a byte-identidade não é afetada.

## Status por fase

### Fase 1 — enum IvfVersion + dispatch (bullet 1) — ✅ byte-idêntico

- `enum IvfVersion { V3..V8 }` + `map_ivf_version` (puro) + `ivf_version(rel)` em `page/ivf.rs`; if-ladder de
  `scan_ivf_structured` → `match ivf_version(rel)` exaustivo; os 5 predicados `ivf_is_v*` removidos.
- **Escopo além do Baseline Context (declarado):** os `ivf_is_v*` tinham callers em `build.rs` também (VACUUM
  path :726 = produção; 5 sítios de `#[pg_test]`), não só o scan como o Baseline mapeou. Todos migrados para
  o enum (`matches!(ivf_version, Ok(V4|V5|V6|V7|V8))` no VACUUM; `== Ok(V{n})` nos testes).
- **Medido:** example `ivf_dispatch_check` OK; não-vacuidade por mutação (gate `<8`→`<4` e `2|3`→`3` ambos
  pegos); **A/B in-PG: `AB_COMPARE_OK`, 6 caminhos v3..v8 byte-idênticos ao baseline**. Build exit 0.
