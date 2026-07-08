# M58 — SIMD (AVX2+FMA) para cosine/inner-product: micro-bench per-candidate

Caracterização (NÃO comparação competitiva) do ganho por-candidato ao vetorizar o hot-path de distância
cosine/IP do scan (`cosine_dist_from_bytes` / `ip_dist_from_bytes`), o eixo dos embeddings reais (OpenAI/Cohere).
Até o M58 só o L2 tinha AVX2; cosine/IP rodavam escalar (o gap P2 do deep-view).

## Micro-bench (per-candidate, dim=768)

`cosine_dist_from_bytes` despachado sob cada branch forçado (`simd_x86::force_for_test`), 200k iterações no
builder pg17 (mesma box, mesmo vetor):

| Kernel | Tempo (200k iters) | Custo/candidato |
|---|---|---|
| escalar | 2.3927 s | ~12.0 µs |
| **AVX2+FMA** | **0.7597 s** | **~3.8 µs** |
| **speedup** | **3.15×** | |

(Reprodução: `cargo pgrx test pg17 cosine_simd_per_candidate` → grava o ratio em `target/m58-speedup.txt` e no server LOG.)

## Correção (recall-neutro)

- **Paridade escalar↔SIMD dentro de eps** (`cosine_and_ip_from_bytes_match_scalar_within_eps_across_dims`),
  ambos os branches (AVX + scalar), dims cobrindo o tail do 8-lane (1/7/8/9/16/17/128/768). Aproximado
  (lane-reduce arredonda diferente, ~1 ULP·√dim), **recall-preserving** — a MESMA regra parity-not-identity do
  kernel L2 SIMD. O operador SQL exato (`<=>`/`<#>`) continua no caminho escalar (`cosine_distance`).
- Os testes cosine existentes (idêntico=0, ortogonal=1, oposto=2, zero-norm) passam inalterados.

## Veredito

- **Ganho por-candidato de 3.15×** no cosine (o eixo exato onde perdíamos ~1.6× de latência 1-cliente para o
  pgvector — `docs/benchmarks/m50-sota-ruler.md`), sem regressão de correção e sem tocar a arquitetura.
- O macro recall×QPS neutro-em-recall a escala é medido pelo benchmark do M57 (que roda cosine) — e o M58 **acelera
  esse próprio benchmark** (era impraticavelmente lento no cosine escalar). Ordem correta: M58 → M57.

## Caveats honestos

- Micro-bench numa box (builder pg17); o speedup absoluto varia com CPU/dim, mas o kernel 8-wide FMA vs escalar é
  o mesmo padrão medido do L2. Aproximado (não bit-idêntico) por design, recall-preserving.
