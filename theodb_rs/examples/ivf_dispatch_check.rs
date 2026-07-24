//! M147 (Fase 1) — standalone check of the PURE IVF version-dispatch (`map_ivf_version`): mapear os 8 bytes do
//! bloco 0 → `IvfVersion`, com fail-fast tipado (sem panic através de C — a lição do M146).
//!
//! Links WITHOUT a PostgreSQL runtime by `#[path]`-including só a lógica pura — mas `map_ivf_version` vive em
//! `page/ivf.rs`, que importa `pg_sys`. Então este example redefine a função pura idêntica e a testa; a
//! equivalência com a de produção é garantida pelo A/B in-PG (`ab_scan_versions.sh`) + code review. Convenção
//! do crate (`cargo test`/`cargo pgrx test` não linkam). Este binário É o teste: panica em qualquer invariante.
//!
//! Run: `cargo run --example ivf_dispatch_check`

const IVF_STRUCT_MAGIC: u32 = 0x5449_5653; // "TIVS" — idêntico a page/ivf.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IvfVersion {
    V3,
    V4,
    V5,
    V6,
    V7,
    V8,
}

// CÓPIA VERIFICADA de `page::ivf::map_ivf_version` (a de produção importa pg_sys via o módulo pai). Qualquer
// divergência entre esta e a de produção é pega pelo A/B in-PG (o dispatch real produziria top-k diferente).
fn map_ivf_version(m: &[u8]) -> Result<IvfVersion, String> {
    if m.len() < 8 {
        return Err("theodb ivf: truncated header (< 8 bytes)".into());
    }
    if u32::from_le_bytes(m[0..4].try_into().unwrap()) != IVF_STRUCT_MAGIC {
        return Err("theodb ivf: block 0 is not an IVF-structured index".into());
    }
    match u32::from_le_bytes(m[4..8].try_into().unwrap()) {
        2 | 3 => Ok(IvfVersion::V3),
        4 => Ok(IvfVersion::V4),
        5 => Ok(IvfVersion::V5),
        6 => Ok(IvfVersion::V6),
        7 => Ok(IvfVersion::V7),
        8 => Ok(IvfVersion::V8),
        other => Err(format!("theodb ivf: unknown format version {other}")),
    }
}

/// Monta um bloco 0 com magic TIVS + o discriminante `ver` (little-endian), com `extra` bytes de padding.
fn block0(ver: u32, extra: usize) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&IVF_STRUCT_MAGIC.to_le_bytes());
    b.extend_from_slice(&ver.to_le_bytes());
    b.extend(std::iter::repeat(0u8).take(extra));
    b
}

fn main() {
    // EDGE — cada versão válida mapeia para o seu enum.
    assert_eq!(map_ivf_version(&block0(3, 0)), Ok(IvfVersion::V3));
    assert_eq!(map_ivf_version(&block0(4, 0)), Ok(IvfVersion::V4));
    assert_eq!(map_ivf_version(&block0(5, 0)), Ok(IvfVersion::V5));
    assert_eq!(map_ivf_version(&block0(6, 0)), Ok(IvfVersion::V6));
    assert_eq!(map_ivf_version(&block0(7, 0)), Ok(IvfVersion::V7));
    assert_eq!(map_ivf_version(&block0(8, 0)), Ok(IvfVersion::V8));

    // EC-2 — o discriminante 2 (v2 legado, M34) mapeia para V3 (mesmo corpo de scan; auto-migrado no fold).
    // Sem isto, um índice v2 legado — hoje lido via o else — regrediria para Err.
    assert_eq!(map_ivf_version(&block0(2, 0)), Ok(IvfVersion::V3), "v2 legado → V3 (EC-2)");

    // NEGATIVE (EC-2) — versão desconhecida num índice com magic TIVS é Err tipado, não silêncio nem panic.
    assert!(map_ivf_version(&block0(99, 0)).is_err(), "versão desconhecida → Err");
    assert!(map_ivf_version(&block0(1, 0)).is_err(), "v1 (pré-M34) não é suportado → Err");
    assert!(map_ivf_version(&block0(0, 0)).is_err(), "versão 0 → Err");

    // NEGATIVE (EC-3) — bloco com magic mas < 8 bytes é Err, NÃO panica no try_into().unwrap() (seria XX000).
    let mut short = IVF_STRUCT_MAGIC.to_le_bytes().to_vec(); // só 4 bytes (magic, sem discriminante)
    assert!(map_ivf_version(&short).is_err(), "bloco de 4 bytes → Err (EC-3)");
    short.extend_from_slice(&[0u8, 0, 0]); // 7 bytes — ainda < 8
    assert!(map_ivf_version(&short).is_err(), "bloco de 7 bytes → Err (EC-3)");
    // a borda do gate `< 8`: exatamente 8 bytes com magic + discriminante válido é aceito (já coberto acima
    // por block0(4,0) que tem 8 bytes — a asserção seguinte confirma que 8 é o mínimo aceito).
    assert_eq!(block0(4, 0).len(), 8, "block0 sem padding tem exatamente 8 bytes");
    assert!(map_ivf_version(&block0(4, 0)).is_ok(), "8 bytes exatos → Ok (borda do gate)");

    // NEGATIVE — magic errado (não-TIVS) é Err.
    let mut wrong_magic = 0xDEAD_BEEFu32.to_le_bytes().to_vec();
    wrong_magic.extend_from_slice(&3u32.to_le_bytes());
    assert!(map_ivf_version(&wrong_magic).is_err(), "magic errado → Err");

    // Vazio / 1 byte não panicam.
    assert!(map_ivf_version(&[]).is_err());
    assert!(map_ivf_version(&[0x53]).is_err());

    println!(
        "IVF_DISPATCH_CHECK_OK — version mapping is strict, v2→V3, and fail-fast on short/unknown headers"
    );
}
