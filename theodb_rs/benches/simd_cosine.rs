//! B-023 — micro-bench do custo por candidato do cosseno, AVX2+FMA contra escalar, dim=768.
//!
//! # Por que ele mora aqui, e não na suíte funcional
//!
//! Ele viveu como `#[pg_test]` em `vec.rs` até 2026-08-13, e o B-023 mediu por que isso não podia continuar:
//!
//! | condição                         | speedup medido |
//! |----------------------------------|----------------|
//! | teste rodando sozinho            | passa          |
//! | após os outros 439 testes        | 0,78×          |
//! | idem, com 3 contêineres ao lado  | 0,66×          |
//!
//! Monotônico com a carga acumulada, não aleatório: AVX2 dispara redução de frequência por licença em CPUs
//! Intel, e num i7-1355U (TDP de 15 W) o efeito cresce conforme a máquina esquenta ao longo da suíte. O teste
//! media a térmica do laptop e reportava como qualidade do kernel — e um vermelho intermitente treina o time a
//! ignorar vermelho, que é o que `rules/testing.md § 6` proíbe ao vedar tempo em teste unitário sem isolamento.
//!
//! A correção anterior (medianas de rodadas alternadas) melhorou a MEDIÇÃO e não removeu a CLASSE: um bench de
//! parede dentro de uma suíte funcional continua sendo um teste que pode falhar por carga da máquina.
//!
//! # O que ele mede, e o que NÃO mede
//!
//! Mede o custo por candidato do `cosine_dist_from_bytes` REAL — o mesmo código do caminho de scan —, sob cada
//! branch de despacho forçado, com `criterion` cuidando de aquecimento, número de amostras e intervalo de
//! confiança. **Não** é um gate: nenhuma asserção reprova aqui. O `criterion` reporta a variância, e é ela que
//! diz se a diferença observada significa alguma coisa naquela máquina.
//!
//! A regressão de CORREÇÃO (SIMD e escalar concordarem) continua coberta na suíte funcional, onde ela é
//! determinística e pertence — só a medição de VELOCIDADE saiu.
//!
//! # Como ele linka sem PostgreSQL
//!
//! `#[path]`-inclui `src/vec/kernels.rs`, que é puro por contrato (zero `crate::`). Foi o [[B-053]] que o
//! extraiu de `vec.rs`, pela mesma razão e no mesmo formato do `ann/scan_core.rs`: benchar o código real, em
//! vez de uma cópia divergente. Sem isso, o `use crate::pg::err_input` de `vec.rs` faria o bench não linkar —
//! e o `Cargo.toml` registra que tentar isso já bloqueou **todos** os `#[pg_test]` do crate no M144.
//!
//! Rodar: `cargo bench --bench simd_cosine`

use criterion::{Criterion, black_box, criterion_group, criterion_main};

#[path = "../src/vec/kernels.rs"]
mod kernels;

use kernels::{cosine_dist_from_bytes, simd_x86};

/// dim=768 é a dimensão de embedding real que o produto serve, não um número redondo.
const DIM: usize = 768;

/// Os mesmos vetores determinísticos do teste que este bench substitui — para que o número seja comparável
/// com o histórico registrado em `wiki/benchmarks/m58-simd-cosine.md`.
fn fixture() -> (Vec<f32>, Vec<u8>) {
    let q: Vec<f32> = (0..DIM).map(|i| ((i * 7 % 13) as f32) * 0.1 - 0.6).collect();
    let c: Vec<f32> = (0..DIM).map(|i| ((i * 5 % 11) as f32) * 0.1 - 0.5).collect();
    let raw: Vec<u8> = c.iter().flat_map(|f| f.to_le_bytes()).collect();
    (q, raw)
}

fn cosine_per_candidate(c: &mut Criterion) {
    let (q, raw) = fixture();
    let mut grupo = c.benchmark_group("cosine_dist_from_bytes");

    // Os dois branches medidos no MESMO grupo, para que o `criterion` os compare com o próprio intervalo de
    // confiança em vez de a gente dividir duas medianas e chamar o quociente de speedup.
    grupo.bench_function("avx2", |b| {
        simd_x86::force_for_test(true);
        b.iter(|| cosine_dist_from_bytes(black_box(&q), black_box(&raw)));
        simd_x86::reset_for_test();
    });
    grupo.bench_function("scalar", |b| {
        simd_x86::force_for_test(false);
        b.iter(|| cosine_dist_from_bytes(black_box(&q), black_box(&raw)));
        simd_x86::reset_for_test();
    });

    grupo.finish();
}

criterion_group!(benches, cosine_per_candidate);
criterion_main!(benches);
