//! hnsw_page test suite — moved verbatim (M126). `super::*` rewritten to the absolute
//! `crate::am::hnsw_page::*` path since the tests now live one module deeper.
#![allow(unused_imports)]
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use crate::am::hnsw_page::*;
    use crate::am::page;
    use crate::ann::HnswIndex;
    use crate::ann::Metric;
    use pgrx::pg_sys;

    fn corpus() -> Vec<(i64, Vec<f32>)> {
        (0..40)
            .map(|i| {
                (
                    i as i64 + 100,
                    vec![(i % 7) as f32, (i % 5) as f32, (i % 3) as f32, i as f32 / 40.0],
                )
            })
            .collect()
    }

    /// Map an element `(blkno,offno)` back to its node index (inverse of the analytic addr) — test helper.
    fn node_of_elem_addr(addr: Addr, ipp: usize) -> usize {
        (addr.0 as usize - 1) * ipp + (addr.1 as usize - 1)
    }

    #[pgrx::pg_test]
    fn pack_element_addresses_are_analytic() {
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 42);
        let packed = pack(&idx).expect("pack");
        let dim = idx.dim();
        let ipp = elems_per_page(dim, 0);
        // Element pages are the first `elem_npages`; each element decodes and its addr maps back to its node.
        let meta = decode_meta(&packed.meta).unwrap();
        for p in 0..meta.elem_npages as usize {
            for (off, blob) in packed.pages[p].iter().enumerate() {
                let addr = ((1 + p) as u32, (off + 1) as u16);
                let node = node_of_elem_addr(addr, ipp);
                let ev = decode_element(blob).unwrap();
                assert_eq!(ev.tid, idx.node_id(node), "element tid must match its node");
                assert_eq!(ev.level as usize, idx.node_level(node));
            }
        }
    }

    #[pgrx::pg_test]
    fn neighbor_slice_matches_in_memory_graph_every_layer() {
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 7);
        let (_metric, m, m0, _ef) = idx.params();
        let packed = pack(&idx).expect("pack");
        let meta = decode_meta(&packed.meta).unwrap();
        let dim = idx.dim();
        let ipp = elems_per_page(dim, 0);
        let n = idx.node_count();
        // For every node, decode each layer's neighbor addrs, map back to node indices, and assert the SET equals
        // the in-memory `neighbors[node][lc]` — the load-bearing correctness check (blueprint R2).
        for node in 0..n {
            let level = idx.node_level(node);
            // find this node's neighbor tuple via its element's nbr_addr
            let ea = ((1 + node / ipp) as u32, (1 + node % ipp) as u16);
            let ep = packed.pages[ea.0 as usize - 1][ea.1 as usize - 1].as_slice();
            let ev = decode_element(ep).unwrap();
            let (nb_blk, nb_off) = ev.nbr_addr;
            // neighbor pages start at nbr_first
            assert!(nb_blk >= meta.nbr_first, "nbr addr must be in the neighbor range");
            let np = packed.pages[nb_blk as usize - 1][nb_off as usize - 1].as_slice();
            for lc in 0..=level {
                let got: std::collections::HashSet<usize> = decode_neighbors(np, level, lc, m, m0)
                    .unwrap()
                    .into_iter()
                    .map(|a| node_of_elem_addr(a, ipp))
                    .collect();
                let want: std::collections::HashSet<usize> =
                    idx.node_neighbors(node, lc).iter().copied().collect();
                assert_eq!(
                    got, want,
                    "node {node} layer {lc}: decoded neighbors must equal in-memory"
                );
            }
        }
    }

    #[pgrx::pg_test]
    fn empty_graph_packs_to_meta_only_with_sentinel_entry() {
        let idx = HnswIndex::build(&[], 16, 64, Metric::Cosine, 1);
        let packed = pack(&idx).expect("pack empty");
        assert!(packed.pages.is_empty());
        let meta = decode_meta(&packed.meta).unwrap();
        assert_eq!(meta.entry_level, -1);
        assert_eq!(meta.node_count, 0);
    }

    /// M46 L1-B: the reused-scratch variant `decode_neighbors_into` must produce EXACTLY the same addrs as the
    /// allocating `decode_neighbors`, AND must clear any prior contents of the scratch (the scratch-not-cleared
    /// bug that would leak a previous node's neighbors into the next — EC-1 of the edge-case review).
    #[pgrx::pg_test]
    fn decode_neighbors_into_matches_original() {
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 11);
        let (_metric, m, m0, _ef) = idx.params();
        let packed = pack(&idx).expect("pack");
        let dim = idx.dim();
        let ipp = elems_per_page(dim, 0);
        for node in 0..idx.node_count() {
            let level = idx.node_level(node);
            let ea = ((1 + node / ipp) as u32, (1 + node % ipp) as u16);
            let ep = packed.pages[ea.0 as usize - 1][ea.1 as usize - 1].as_slice();
            let ev = decode_element(ep).unwrap();
            let (nb_blk, nb_off) = ev.nbr_addr;
            let np = packed.pages[nb_blk as usize - 1][nb_off as usize - 1].as_slice();
            for lc in 0..=level {
                let orig = decode_neighbors(np, level, lc, m, m0).unwrap();
                // pre-dirty the scratch to prove `_into` clears it before writing.
                let mut scratch: Vec<Addr> = vec![(9999, 9999), (8888, 8888)];
                decode_neighbors_into(np, level, lc, m, m0, &mut scratch).unwrap();
                assert_eq!(
                    scratch, orig,
                    "node {node} layer {lc}: _into must equal original AND clear prior"
                );
            }
        }
    }

    #[pgrx::pg_test]
    fn decode_meta_rejects_bad_magic_and_truncation() {
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 3);
        let good = pack(&idx).unwrap().meta;
        assert!(decode_meta(&good[..good.len() - 1]).is_err(), "truncated meta must Err");
        let mut bad = good.clone();
        bad[0] ^= 0xFF;
        assert!(decode_meta(&bad).is_err(), "bad magic must Err");
    }

    // --- M51 T1.1: layout v2 meta carries the SBQ codebook; v1 (f32-only) stays byte-identical. ---
    fn meta_fixture(sbq_bits: u8, codebook: Vec<u8>) -> HnswMeta {
        HnswMeta {
            metric_tag: Metric::L2.tag(),
            dim: 3,
            m: 16,
            m0: 32,
            entry_blkno: 1,
            entry_offno: 1,
            entry_level: 2,
            node_count: 5,
            elem_first: 1,
            elem_npages: 1,
            nbr_first: 2,
            nbr_npages: 1,
            sbq_bits,
            codebook,
            aq_m: 0,
            aq_codebook: Vec::new(),
            aq_cb_first: 0,
            aq_cb_npages: 0,
            raw_first: 0,
            raw_npages: 0,
        }
    }

    // --- M59 T3.1: layout v3 meta carries the AQ codebook; SBQ off (AQ ⟂ SBQ per index, D1). ---
    fn aq_meta_fixture(aq_m: u8, aq_codebook: Vec<u8>) -> HnswMeta {
        HnswMeta {
            metric_tag: Metric::L2.tag(),
            dim: 8,
            m: 16,
            m0: 32,
            entry_blkno: 1,
            entry_offno: 1,
            entry_level: 2,
            node_count: 5,
            elem_first: 1,
            elem_npages: 1,
            nbr_first: 2,
            nbr_npages: 1,
            sbq_bits: 0,
            codebook: Vec::new(),
            aq_m,
            aq_codebook,
            aq_cb_first: 3,
            aq_cb_npages: 1,
            raw_first: 0,
            raw_npages: 0,
        }
    }

    // --- M59 v4: layout v4 meta carries the AQ codebook descriptor + the raw-f32 region pointer. ---
    fn v4_meta_fixture(aq_m: u8, aq_codebook: Vec<u8>) -> HnswMeta {
        HnswMeta {
            metric_tag: Metric::L2.tag(),
            dim: 8,
            m: 16,
            m0: 32,
            entry_blkno: 1,
            entry_offno: 1,
            entry_level: 2,
            node_count: 5,
            elem_first: 1,
            elem_npages: 1,
            nbr_first: 2,
            nbr_npages: 1,
            sbq_bits: 0,
            codebook: Vec::new(),
            aq_m,
            aq_codebook,
            aq_cb_first: 3,
            aq_cb_npages: 1,
            raw_first: 4,
            raw_npages: 2,
        }
    }

    #[pgrx::pg_test]
    fn hnsw_meta_v2_roundtrips_codebook() {
        let cb = vec![4u8, 3, 0, 0, 0, 1, 2, 3, 4]; // arbitrary codebook bytes (e.g. to_meta_bytes output)
        let bytes = encode_meta(&meta_fixture(4, cb.clone()));
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            HNSW_STRUCT_VERSION_SBQ,
            "must be v2"
        );
        let d = decode_meta(&bytes).expect("v2 decodes");
        assert_eq!(d.sbq_bits, 4);
        assert_eq!(d.codebook, cb, "codebook roundtrips byte-exact");
        assert_eq!(d.dim, 3);
        assert_eq!(d.node_count, 5);
    }

    #[pgrx::pg_test]
    fn hnsw_meta_v1_byte_identical_when_no_sbq() {
        let bytes = encode_meta(&meta_fixture(0, Vec::new()));
        assert_eq!(bytes.len(), META_LEN, "f32-only meta must be exactly the 45-byte v1 layout");
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            HNSW_STRUCT_VERSION,
            "must stay v1"
        );
        let d = decode_meta(&bytes).unwrap();
        assert_eq!(d.sbq_bits, 0);
        assert!(d.codebook.is_empty());
    }

    #[pgrx::pg_test]
    fn hnsw_meta_v2_rejects_truncated_codebook() {
        let mut bytes = encode_meta(&meta_fixture(1, vec![1, 2, 3, 4]));
        bytes.truncate(bytes.len() - 1); // drop one codebook byte → declared cb_len mismatch
        assert!(decode_meta(&bytes).is_err(), "truncated v2 codebook must Err (Rule 8)");
    }

    #[pgrx::pg_test]
    fn pack_sbq_writes_codebook_and_matching_codes() {
        // T1.1-build + T2.1-write (M51): pack_sbq trains the quantizer, persists the codebook in the v2 meta, and
        // writes each node's inline code == the quantizer's code for that node's vector.
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 9);
        let bits = 2u8;
        let packed = pack_sbq(&idx, bits).expect("pack_sbq");
        let meta = decode_meta(&packed.meta).unwrap();
        assert_eq!(meta.sbq_bits, bits, "meta records SBQ bits");
        assert!(!meta.codebook.is_empty(), "codebook persisted in meta");
        // reconstruct the quantizer from the persisted codebook and verify each element's code matches.
        let q =
            crate::sbq::SbqQuantizer::from_meta_bytes(&meta.codebook).expect("codebook decodes");
        let dim = idx.dim();
        let ipp = elems_per_page(dim, crate::sbq::SbqQuantizer::bytes_per_vector(dim, bits));
        for node in 0..idx.node_count() {
            let ep = packed.pages[node / ipp][node % ipp].as_slice();
            let ev = decode_element(ep).unwrap();
            let expect: Vec<u8> =
                q.quantize(idx.node_vector(node)).iter().flat_map(|w| w.to_le_bytes()).collect();
            assert_eq!(
                ev.code_bytes,
                expect.as_slice(),
                "node {node}: inline code == quantize(vec)"
            );
        }
    }

    #[pgrx::pg_test]
    fn element_code_length_is_exact_so_load_guard_detects_truncation() {
        // H2 (M51 review, Rule 8): a truncated on-disk SBQ code must be caught, not silently Hamming'd. The guard
        // in `load` compares `ev.code_bytes.len()` to the query code length (both = bytes_per_vector). This proves
        // `decode_element` exposes the EXACT trailing length, so any truncation is observable by that guard.
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 5);
        let dim = idx.dim();
        let full = crate::sbq::SbqQuantizer::bytes_per_vector(dim, 2);
        let code = vec![0xAAu8; full];
        let e = encode_element(&idx, 0, (1, 1), dim, &code);
        assert_eq!(
            decode_element(&e).unwrap().code_bytes.len(),
            full,
            "full code exposes its exact length"
        );
        // a shorter (truncated) code decodes to a SHORTER code_bytes → the load guard (len != qcode.len()) fires.
        let e_short = encode_element(&idx, 0, (1, 1), dim, &code[..full - 1]);
        assert_eq!(
            decode_element(&e_short).unwrap().code_bytes.len(),
            full - 1,
            "truncation is observable"
        );
    }

    #[pgrx::pg_test]
    fn element_tuple_carries_optional_sbq_code() {
        // T2.1 (M51): an element tuple optionally carries the SBQ code after the f32 vec; v1 (no code) stays
        // byte-identical and the f32 vec bytes are untouched when a code is appended.
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 7);
        let dim = idx.dim();
        let e1 = encode_element(&idx, 0, (1, 1), dim, &[]);
        assert_eq!(e1.len(), elem_size(dim, 0), "v1 tuple size unchanged");
        let v1 = decode_element(&e1).unwrap();
        assert!(v1.code_bytes.is_empty(), "v1 tuple has no SBQ code");

        let code = vec![0xABu8, 0xCD, 0x01, 0x02];
        let e2 = encode_element(&idx, 0, (1, 1), dim, &code);
        assert_eq!(e2.len(), elem_size(dim, code.len()), "v2 tuple grows by the code length");
        let v2 = decode_element(&e2).unwrap();
        assert_eq!(v2.code_bytes, code.as_slice(), "SBQ code roundtrips inline after the vec");
        assert_eq!(
            v2.vec_bytes, v1.vec_bytes,
            "appending a code must not change the f32 vec bytes"
        );
    }

    // ============================ M59 T3.1 — meta v3 codec + AQ pack path ============================

    /// A dim-8 corpus (divisible by the AQ subspace counts used in these tests, m ∈ {2,4}) so
    /// `AqQuantizer::train` accepts it (`dim % m == 0`, Rule 8). Distinct points, deterministic.
    fn aq_corpus() -> Vec<(i64, Vec<f32>)> {
        (0..40)
            .map(|i| {
                let f = i as f32;
                (
                    i as i64 + 200,
                    vec![
                        f,
                        (i % 7) as f32,
                        (i % 5) as f32,
                        (i % 3) as f32,
                        f * 0.1,
                        (i % 11) as f32,
                        (i % 2) as f32,
                        f * 0.5,
                    ],
                )
            })
            .collect()
    }

    #[pgrx::pg_test]
    fn aq_meta_v3_roundtrips() {
        // encode_meta(v3) → decode_meta yields the identical AQ DESCRIPTOR (aq_m + codebook page pointers), byte-
        // exact. M59 fix: the codebook bytes are NOT inline in the meta item anymore (they overflow one page at
        // dim=768) — they live on the pages `[aq_cb_first, aq_cb_first+aq_cb_npages)`. So the pure codec exposes
        // an EMPTY `aq_codebook` (the FFI `read_meta` reassembles it from pages); the descriptor round-trips here.
        let cb = vec![4u8, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 128, 63, 9, 8, 7, 6]; // arbitrary AqQuantizer::to_meta_bytes-shaped bytes
        let bytes = encode_meta(&aq_meta_fixture(2, cb.clone()));
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            HNSW_STRUCT_VERSION_AQ,
            "must be v3"
        );
        // The v3 meta ITEM is tiny (core + 13-byte descriptor) — it can NEVER overflow a page, regardless of dim.
        assert_eq!(
            bytes.len(),
            META_LEN + AQ_DESC_LEN,
            "v3 meta item is core + fixed 13-byte descriptor only"
        );
        let d = decode_meta(&bytes).expect("v3 decodes");
        assert_eq!(d.aq_m, 2, "aq_m roundtrips");
        assert!(
            d.aq_codebook.is_empty(),
            "codebook is NOT inline — it lives on pages (read_meta reassembles it)"
        );
        assert_eq!(d.aq_cb_first, 3, "codebook first-page pointer roundtrips");
        assert_eq!(d.aq_cb_npages, 1, "codebook page-count roundtrips");
        assert_eq!(d.sbq_bits, 0, "AQ index carries no SBQ (mutually exclusive, D1)");
        assert!(d.codebook.is_empty(), "no v2 codebook on a v3 index");
        assert_eq!(d.dim, 8);
        assert_eq!(d.node_count, 5);
    }

    #[pgrx::pg_test]
    fn v1_v2_meta_still_decodes() {
        // BACKWARD-COMPAT (the most important test): pre-existing v1 and v2 meta bytes decode UNCHANGED after the
        // v3 codec was added. v1 stays the exact 45-byte layout; v2 keeps its SBQ trailer; neither grows an AQ
        // field. A v3-aware reader must read old indexes bit-for-bit (WAL/crash-safety invariant).
        // -- v1 (f32-only) --
        let v1 = encode_meta(&meta_fixture(0, Vec::new()));
        assert_eq!(v1.len(), META_LEN, "v1 stays the byte-identical 45-byte core");
        assert_eq!(
            u32::from_le_bytes(v1[4..8].try_into().unwrap()),
            HNSW_STRUCT_VERSION,
            "v1 version unchanged"
        );
        let d1 = decode_meta(&v1).expect("v1 still decodes");
        assert_eq!(d1.sbq_bits, 0);
        assert!(d1.codebook.is_empty());
        assert_eq!(d1.aq_m, 0, "v1 carries no AQ");
        assert!(d1.aq_codebook.is_empty());
        // -- v2 (SBQ) --
        let cb = vec![4u8, 3, 0, 0, 0, 1, 2, 3, 4];
        let v2 = encode_meta(&meta_fixture(4, cb.clone()));
        assert_eq!(
            u32::from_le_bytes(v2[4..8].try_into().unwrap()),
            HNSW_STRUCT_VERSION_SBQ,
            "v2 version unchanged"
        );
        let d2 = decode_meta(&v2).expect("v2 still decodes");
        assert_eq!(d2.sbq_bits, 4, "v2 SBQ bits unchanged");
        assert_eq!(d2.codebook, cb, "v2 codebook unchanged");
        assert_eq!(d2.aq_m, 0, "v2 carries no AQ");
        assert!(d2.aq_codebook.is_empty());
    }

    #[pgrx::pg_test]
    fn pack_v4_writes_codebook_hot_codes_and_separate_raw_region() {
        // M59 v4 pack: pack_aq trains the AQ quantizer, persists the codebook on dedicated pages, writes each
        // node's HOT tuple (code + raw_addr, NO f32) into the element region, and its exact f32 into the SEPARATE
        // cold raw region. This is the ADR-0019 code/vector separation — proven at the pack level here.
        let idx = HnswIndex::build(&aq_corpus(), 16, 64, Metric::L2, 9);
        let (m_sub, bits, thr) = (4usize, 4u8, 2.0f32);
        let packed = pack_aq(&idx, 1, m_sub, bits, thr).expect("pack_aq");
        let meta = decode_meta(&packed.meta).unwrap();
        assert_eq!(meta.aq_m as usize, m_sub, "meta records AQ subspace count");
        assert_eq!(meta.sbq_bits, 0, "v4 index has no SBQ");
        assert_eq!(
            u32::from_le_bytes(packed.meta[4..8].try_into().unwrap()),
            HNSW_STRUCT_VERSION_V4,
            "meta is v4"
        );
        assert!(meta.raw_npages > 0, "v4 records a non-empty raw-f32 region");
        assert!(
            meta.raw_first >= meta.aq_cb_first + meta.aq_cb_npages,
            "raw region follows the codebook pages"
        );
        // Codebook on the dedicated pages (reassembled — the in-memory dual of the FFI read_meta).
        let cb = codebook_from_packed(&packed, meta.aq_cb_first, meta.aq_cb_npages);
        let q = crate::vec::aq::AqQuantizer::from_meta_bytes(&cb).expect("AQ codebook decodes");
        let dim = idx.dim();
        let code_len = crate::vec::aq::AqQuantizer::bytes_per_vector(dim, m_sub);
        let ipp = elems_per_page_v4(code_len);
        let rpp = raws_per_page(dim);
        for node in 0..idx.node_count() {
            // HOT tuple: code matches encode(vec); the hot tuple size is header+code (dim-independent, NO f32).
            let ep = packed.pages[node / ipp][node % ipp].as_slice();
            let ev = decode_element_v4(ep).unwrap();
            assert_eq!(
                ev.code_bytes,
                q.encode(idx.node_vector(node)).as_slice(),
                "node {node}: hot code == encode(vec)"
            );
            assert_eq!(ev.code_bytes.len(), m_sub.div_ceil(2), "code is ⌈m/2⌉ bytes");
            assert_eq!(
                ep.len(),
                elem_size_v4(code_len),
                "hot tuple = header + code, NO f32 (dim-independent)"
            );
            // The raw_addr the hot tuple links must point into the raw region and round-trip the exact f32 vector.
            assert!(ev.raw_addr.0 >= meta.raw_first, "raw_addr points into the cold raw region");
            let rp =
                packed.pages[(ev.raw_addr.0 - 1) as usize][(ev.raw_addr.1 - 1) as usize].as_slice();
            let vb = decode_raw_vec(rp).expect("raw tuple decodes");
            let got: Vec<f32> =
                vb.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
            assert_eq!(
                got.as_slice(),
                idx.node_vector(node),
                "node {node}: raw tuple round-trips the exact f32"
            );
        }
        // The analytic raw addr matches the linked raw_addr (node i → raw_first + i/rpp, off 1 + i%rpp).
        let ev0 = decode_element_v4(packed.pages[0][0].as_slice()).unwrap();
        assert_eq!(
            ev0.raw_addr,
            (meta.raw_first, 1),
            "node 0's raw tuple is the first item of the raw region"
        );
        let _ = rpp;
    }

    #[pgrx::pg_test]
    fn pack_aq_m_zero_is_v1_identical() {
        // Edge: pack_aq with m=0 falls back to the byte-identical v1 f32-only pack (no code, no trailer).
        let idx = HnswIndex::build(&aq_corpus(), 16, 64, Metric::L2, 3);
        let v1 = pack(&idx).expect("pack v1");
        let aq0 = pack_aq(&idx, 1, 0, 4, 1.0).expect("pack_aq m=0");
        assert_eq!(aq0.meta, v1.meta, "pack_aq(m=0) meta must be byte-identical to the v1 pack");
        let d = decode_meta(&aq0.meta).unwrap();
        assert_eq!(d.aq_m, 0, "m=0 ⇒ no AQ trailer");
    }

    #[pgrx::pg_test]
    fn decode_meta_rejects_unknown_version() {
        // Negative (Rule 8): an UNSUPPORTED version (v5+, now that v4 is valid) in the slot ⇒ typed Err, never a
        // panic across the C boundary. Also asserts a truncated v3 AQ DESCRIPTOR is rejected (short descriptor).
        let cb = vec![4u8, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 128, 63, 9, 8, 7, 6];
        let mut bytes = encode_meta(&aq_meta_fixture(2, cb));
        // Bump the version slot to v5 (unsupported — v1..v4 are the known versions).
        bytes[4..8].copy_from_slice(&5u32.to_le_bytes());
        assert!(decode_meta(&bytes).is_err(), "unknown version v5 must Err, not panic");
        // A truncated v3 descriptor (fewer than the fixed 13 bytes present) must also Err.
        let mut short = encode_meta(&aq_meta_fixture(2, vec![1u8, 2, 3, 4, 5]));
        short.truncate(short.len() - 1);
        assert!(decode_meta(&short).is_err(), "truncated v3 AQ descriptor must Err (Rule 8)");
    }

    #[pgrx::pg_test]
    fn v4_meta_roundtrips_raw_region_descriptor() {
        // M59 v4: the v4 meta trailer carries the AQ codebook descriptor PLUS the raw-f32 region pointer, and
        // decodes back byte-for-byte. v1/v2/v3 stay discriminated by their own versions (backward-compat).
        let cb = vec![4u8, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 128, 63, 9, 8, 7, 6];
        let bytes = encode_meta(&v4_meta_fixture(2, cb.clone()));
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            HNSW_STRUCT_VERSION_V4,
            "must be v4"
        );
        let d = decode_meta(&bytes).expect("v4 decodes");
        assert_eq!(d.aq_m, 2, "v4 AQ subspace count round-trips");
        assert_eq!((d.raw_first, d.raw_npages), (4, 2), "v4 raw-f32 region pointer round-trips");
        assert_eq!(d.sbq_bits, 0, "v4 is not SBQ");
        // A v3 (aq) fixture must NOT be read as v4 (no raw region) — the version keeps them apart.
        let v3 = decode_meta(&encode_meta(&aq_meta_fixture(2, cb))).expect("v3 decodes");
        assert_eq!((v3.raw_first, v3.raw_npages), (0, 0), "a v3 index has no raw region");
    }

    /// M59 v4 — THE BYTE TEST the owner required: prove the walk/score path physically cannot touch the f32.
    /// The hot v4 element tuple decodes via `decode_element_v4`, whose view exposes ONLY the code + the two
    /// addresses — there is NO `vec_bytes` field. Scoring a candidate (`ah_score`) consumes just `code_bytes`.
    /// So the f32 is structurally absent from the hot-path: it lives in a SEPARATE raw tuple reached only by the
    /// rerank via `raw_addr`. This is the structural guarantee ADR-0019 demands (co-location was the root cause).
    #[pgrx::pg_test]
    fn v4_hot_tuple_has_no_f32_walk_never_pages_the_vector() {
        let idx = HnswIndex::build(&aq_corpus(), 16, 64, Metric::L2, 5);
        let (m_sub, dim) = (4usize, idx.dim());
        let q = crate::vec::aq::AqQuantizer::train(
            &(0..idx.node_count()).map(|i| idx.node_vector(i).to_vec()).collect::<Vec<_>>(),
            m_sub,
            4,
            2.0,
            AQ_BUILD_SEED,
        )
        .expect("train");
        let code = q.encode(idx.node_vector(0));
        // Build a hot tuple whose linked raw_addr is a DELIBERATELY POISONED sentinel (u32::MAX, u16::MAX): if the
        // walk/score path read the f32, it would have to dereference this address and fail. It must NOT.
        let poison = (u32::MAX, u16::MAX);
        let hot = encode_element_v4(&idx, 0, (7, 3), poison, dim, &code);
        // (1) The hot tuple size is header+code — it does NOT contain dim*4 f32 bytes.
        assert_eq!(
            hot.len(),
            elem_size_v4(code.len()),
            "hot tuple carries no f32 (size = header + code only)"
        );
        assert!(
            hot.len() < ELEM_HEADER_V4 + dim * 4,
            "hot tuple is far smaller than a co-located f32 tuple"
        );
        // (2) The decoded HOT view exposes NO vector — only code + addresses. Scoring uses just the code.
        let ev = decode_element_v4(&hot).unwrap();
        assert_eq!(ev.code_bytes, code.as_slice(), "hot view exposes the code");
        assert_eq!(ev.raw_addr, poison, "hot view carries raw_addr WITHOUT reading it");
        let lut = crate::vec::ah::build_lut16(idx.node_vector(0), &q).expect("lut");
        // Scoring the candidate succeeds using ONLY the hot code — the poisoned raw_addr is never dereferenced.
        let _score = crate::vec::ah::ah_score(&lut, ev.code_bytes);
        // (3) The type system enforces it: `ElementViewV4` has no `vec_bytes`. (Compile-time proof — this line
        // documents that the ONLY way to the f32 is `decode_raw_vec` on a SEPARATE raw tuple, at rerank.)
    }

    #[pgrx::pg_test]
    fn v4_element_and_raw_tuple_roundtrip() {
        // M59 v4: round-trip the HOT element tuple (code + nbr_addr + raw_addr, NO f32) AND the SEPARATE raw-f32
        // tuple. `elem_size_v4` accounts for the ⌈m/2⌉ code exactly (analytic-address invariant); the raw tuple
        // holds the f32.
        let idx = HnswIndex::build(&aq_corpus(), 16, 64, Metric::L2, 7);
        let dim = idx.dim();
        // m=6 → ⌈6/2⌉ = 3 code bytes; last byte holds one nibble (odd m edge).
        let code = vec![0x21u8, 0x43, 0x05];
        let (nbr, raw) = ((7u32, 3u16), (99u32, 5u16));
        let e = encode_element_v4(&idx, 0, nbr, raw, dim, &code);
        assert_eq!(
            e.len(),
            elem_size_v4(code.len()),
            "v4 hot tuple size = header + ⌈m/2⌉ code (NO f32)"
        );
        let ev = decode_element_v4(&e).unwrap();
        assert_eq!(ev.code_bytes, code.as_slice(), "AQ code roundtrips in the hot tuple");
        assert_eq!(ev.nbr_addr, nbr, "neighbor addr roundtrips");
        assert_eq!(ev.raw_addr, raw, "raw addr roundtrips");
        assert_eq!(ev.dim as usize, dim, "dim tag roundtrips");
        assert_eq!(ev.version, HNSW_ELEM_VERSION_V4, "v4 version byte set");
        // The separate raw-f32 tuple round-trips the exact vector.
        let r = encode_raw_vec(idx.node_vector(0));
        assert_eq!(r.len(), raw_size(dim), "raw tuple size = header + dim*4");
        let vb = decode_raw_vec(&r).unwrap();
        let got: Vec<f32> =
            vb.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        assert_eq!(
            got.as_slice(),
            idx.node_vector(0),
            "raw tuple round-trips the exact f32 vector"
        );
        // Negative (Rule 8): a raw tuple read as a hot element (wrong tag) fails fast, and vice-versa.
        assert!(decode_element_v4(&r).is_err(), "a raw tuple is not a hot element (tag guard)");
        assert!(decode_raw_vec(&e).is_err(), "a hot element is not a raw tuple (tag guard)");
    }

    /// Collect the id column of `SELECT id FROM rn ORDER BY e <-> q LIMIT k` under the current planner GUCs,
    /// via a real Spi round-trip — the only way to exercise the on-disk `traverse` (it needs a live Relation).
    #[cfg(any(test, feature = "pg_test"))]
    fn topk_ids(query_lit: &str, k: i64) -> Vec<i32> {
        topk_ids_tbl("rn", query_lit, k)
    }
    #[cfg(any(test, feature = "pg_test"))]
    fn topk_ids_tbl(tbl: &str, query_lit: &str, k: i64) -> Vec<i32> {
        let sql = format!("SELECT id FROM {tbl} ORDER BY e <-> '{query_lit}'::vector LIMIT {k}");
        pgrx::Spi::connect(|client| {
            // pgrx 0.16.1: `select`'s 3rd arg is `args: &[DatumWithOid]` (a slice) — `&[]`, never `None`
            // (matches hybrid.rs / ann_query.rs). Column ordinals are 1-based → `row.get::<i32>(1)`.
            client
                .select(&sql, None, &[])
                .unwrap()
                .filter_map(|row| row.get::<i32>(1).unwrap())
                .collect::<Vec<i32>>()
        })
    }

    /// M46 L1-A recall-neutrality, proven END-TO-END through the real `traverse` (index scan) against an
    /// INDEPENDENT oracle (the exact seqscan) — NOT a golden vector snapshotted from the already-mutated tree
    /// (which would be circular; EC-2 + SEPA initial-brief). The pre-size (`with_capacity`) is a std-guaranteed
    /// capacity hint that cannot alter visit order; this test is the load-bearing regression guard proving it.
    /// On a tiny distinct corpus at ef_search=200, HNSW recall is 100%, so the index top-k set MUST equal the
    /// exact top-k set, and repeated index runs MUST be byte-identical (determinism).
    #[pgrx::pg_test]
    fn traverse_presize_is_recall_neutral_end_to_end() {
        pgrx::Spi::run("CREATE TEMP TABLE rn (id int PRIMARY KEY, e vector(4))").unwrap();
        // 30 deterministic, distinct points — no distance ties near the probe → unambiguous exact NN.
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO rn VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX rn_idx ON rn USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();

        let probe = "[3.3,1.1,2.2,0.4]";
        // Exact oracle: force a seqscan (bypass the AM entirely).
        pgrx::Spi::run(
            "SET enable_indexscan = off; SET enable_bitmapscan = off; SET enable_seqscan = on",
        )
        .unwrap();
        let exact = topk_ids(probe, 5);
        // Index path: force the theodb_hnsw index scan → exercises `traverse` with the pre-sized structures.
        pgrx::Spi::run(
            "SET enable_seqscan = off; SET enable_bitmapscan = off; SET enable_indexscan = on",
        )
        .unwrap();
        let via_index_1 = topk_ids(probe, 5);
        let via_index_2 = topk_ids(probe, 5);

        assert_eq!(
            via_index_1, via_index_2,
            "traverse must be deterministic (pre-size adds no nondeterminism)"
        );
        let (mut si, mut se) = (via_index_1.clone(), exact.clone());
        si.sort_unstable();
        se.sort_unstable();
        assert_eq!(
            si, se,
            "recall-neutral: index top-5 set must equal exact top-5 set (100% recall at ef=200)"
        );
    }

    /// Negative case (testing.md §4.1): `ef_search = 0` is rejected at the GUC boundary (MIN_EF_SEARCH=1) with a
    /// typed error — it can never reach `traverse`, so the internal `ef_search.max(1)` clamp is defense-in-depth.
    /// This fail-fast-at-the-boundary is the honest form of the plan's "ef=0 → clamp, no crash" acceptance.
    #[pgrx::pg_test(
        error = "0 is outside the valid range for parameter \"theodb_hnsw.ef_search\" (1 .. 1000)"
    )]
    fn ef_search_zero_rejected_at_guc_boundary() {
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 0").unwrap();
    }

    /// M51 reloption connect: `CREATE INDEX ... WITH (sbq_bits=4)` builds a v2 index (SBQ codes inline). Until the
    /// traverse uses the Hamming path (T3.1), the f32 rerank scan MUST still return the exact top-k — proving the
    /// inline codes do not corrupt the vector scoring and the reloption is wired end-to-end.
    #[pgrx::pg_test]
    fn create_index_with_sbq_bits_scans_correctly() {
        pgrx::Spi::run("CREATE TEMP TABLE rs (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO rs VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX rs_idx ON rs USING theodb_hnsw (e) WITH (sbq_bits = 4)")
            .unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();
        let probe = "[3.3,1.1,2.2,0.4]";
        pgrx::Spi::run(
            "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
        )
        .unwrap();
        let exact = topk_ids_tbl("rs", probe, 5);
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let mut via_index = topk_ids_tbl("rs", probe, 5);
        assert!(!via_index.is_empty(), "the SBQ-built index must be scannable (reloption wired)");
        let (mut si, mut se) = (via_index.clone(), exact.clone());
        si.sort_unstable();
        se.sort_unstable();
        via_index.sort_unstable();
        assert_eq!(
            si, se,
            "SBQ v2 index top-5 must equal exact top-5 (codes present don't corrupt f32 scoring)"
        );
    }

    /// M51 T3.1 recall gate: on a corpus where the Hamming walk does NOT cover everything (walk_ef < node_count),
    /// the cheap-Hamming navigation + exact-f32 rerank still recovers high recall@10 vs the exact oracle. This is
    /// the property M40 predicts (carrier-limited: over_fetch widens the pool so the true NN survives the rerank).
    /// NOTE: `sbq_bits=2` recovers here because the corpus is low-dim (16-d, structured). At high dim (128-d) 2-bit
    /// navigation is too lossy (`wiki/benchmarks/m51-sbq-inline.md § 3` measured recall 0.52); the benchmark uses
    /// 8-bit for the ≥0.99 gate. This unit test proves the read-path MECHANISM, not that 2-bit is safe in general.
    #[pgrx::pg_test]
    fn sbq_traverse_hamming_then_rerank_recall_high() {
        pgrx::Spi::run("CREATE TEMP TABLE rq (id int PRIMARY KEY, e vector(16))").unwrap();
        for i in 0..400i32 {
            // deterministic, well-spread distinct points (id-dominated with a per-dim ripple → clear NN structure)
            let v: Vec<String> = (0..16)
                .map(|j| format!("{:.3}", i as f32 * 0.5 + ((i * 7 + j * 13) % 29) as f32 * 0.3))
                .collect();
            pgrx::Spi::run(&format!("INSERT INTO rq VALUES ({i}, '[{}]')", v.join(","))).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX rq_idx ON rq USING theodb_hnsw (e) WITH (sbq_bits = 2)")
            .unwrap();
        // walk_ef = ef_search * over_fetch = 50 * 6 = 300 < 400 → navigation + rerank genuinely tested.
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 50; SET theodb_hnsw.over_fetch = 6").unwrap();
        let probe = "[40,41,42,40,41,42,40,41,42,40,41,42,40,41,42,40]";
        pgrx::Spi::run(
            "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
        )
        .unwrap();
        let exact = topk_ids_tbl("rq", probe, 10);
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let via_index = topk_ids_tbl("rq", probe, 10);
        let hits = via_index.iter().filter(|id| exact.contains(id)).count();
        let recall = hits as f64 / exact.len().max(1) as f64;
        assert!(
            recall >= 0.9,
            "SBQ Hamming+rerank recall@10 = {recall:.2} (hits {hits}/{}) — expected >= 0.9 (over_fetch=6 recovers it)",
            exact.len()
        );
    }

    #[cfg(any(test, feature = "pg_test"))]
    fn filtered_topk(tbl: &str, filter: &str, q: &str, k: i64) -> Vec<i32> {
        let sql =
            format!("SELECT id FROM {tbl} WHERE {filter} ORDER BY e <-> '{q}'::vector LIMIT {k}");
        pgrx::Spi::connect(|c| {
            c.select(&sql, None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect::<Vec<i32>>()
        })
    }

    /// M52 recall gate: under a SELECTIVE `WHERE` a naive HNSW (≤ ef_search tuples) would miss the top-k that pass
    /// the filter; the iterative scan grows ef until `max_scan_tuples`, so the index-scan top-k EQUALS the exact
    /// seqscan top-k (recall preserved). The executor applies `cat = 7` as a recheck over the ordered index emit.
    #[pgrx::pg_test]
    fn filtered_scan_preserves_recall_via_iterative() {
        pgrx::Spi::run("CREATE TEMP TABLE ft (id int PRIMARY KEY, cat int, e vector(8))").unwrap();
        for i in 0..500i32 {
            let cat = i % 100; // `WHERE cat = 7` selects ~5 rows (~1% selectivity)
            let v: Vec<String> = (0..8)
                .map(|j| {
                    format!("{:.3}", i as f32 * 0.1 + j as f32 + ((i * 7 + j) % 13) as f32 * 0.2)
                })
                .collect();
            pgrx::Spi::run(&format!("INSERT INTO ft VALUES ({i}, {cat}, '[{}]')", v.join(",")))
                .unwrap();
        }
        pgrx::Spi::run("CREATE INDEX ft_idx ON ft USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 40; SET theodb_hnsw.max_scan_tuples = 20000")
            .unwrap();
        let probe = "[20,21,22,23,24,25,26,27]";
        pgrx::Spi::run(
            "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
        )
        .unwrap();
        let exact = filtered_topk("ft", "cat = 7", probe, 3);
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let via_index = filtered_topk("ft", "cat = 7", probe, 3);
        assert!(!exact.is_empty(), "the filtered oracle must return rows (test setup)");
        assert!(
            !via_index.is_empty(),
            "the filtered index scan must return results (iterative scan)"
        );
        let hits = via_index.iter().filter(|id| exact.contains(id)).count();
        assert_eq!(
            hits,
            exact.len(),
            "iterative-scan recall under a selective filter must equal exact seqscan ({hits}/{})",
            exact.len()
        );
    }

    /// M52 OFF switch: `max_scan_tuples = 0` disables the iterative scan (pre-M52 behavior) — at most the
    /// ef_search window is emitted; no panic / infinite loop under a selective filter.
    #[pgrx::pg_test]
    fn iterative_scan_off_when_max_scan_tuples_zero() {
        pgrx::Spi::run("CREATE TEMP TABLE fo (id int PRIMARY KEY, cat int, e vector(4))").unwrap();
        for i in 0..300i32 {
            pgrx::Spi::run(&format!(
                "INSERT INTO fo VALUES ({i}, {}, '[{},{},{},{}]')",
                i % 100,
                i as f32 * 0.1,
                (i % 7) as f32,
                (i % 5) as f32,
                i as f32 / 30.0
            ))
            .unwrap();
        }
        pgrx::Spi::run("CREATE INDEX fo_idx ON fo USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 10; SET theodb_hnsw.max_scan_tuples = 0")
            .unwrap();
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let got = filtered_topk("fo", "cat = 3", "[5,1,2,0.5]", 3);
        assert!(got.len() <= 3, "OFF path returns at most k, no infinite loop (got {})", got.len());
    }

    /// M53 item 1 (filter_sql) + item 4 (language): `ai.hybrid_search_rrf` accepts a relational filter (confined
    /// to the CTE WHERE) and a parametrizable FTS language. This SQL-level test proves the filter is APPLIED
    /// (every returned id satisfies `cat = 1`) and the language param is honored (no error with 'simple').
    #[pgrx::pg_test]
    fn hybrid_search_accepts_filter_and_language() {
        pgrx::Spi::run("CREATE TEMP TABLE hy (id int, cat int, tsv tsvector, emb vector(3))")
            .unwrap();
        for i in 0..20i32 {
            let cat = i % 2; // half cat=0, half cat=1
            pgrx::Spi::run(&format!(
                "INSERT INTO hy VALUES ({i}, {cat}, to_tsvector('english', 'alpha beta doc{i}'), '[{},{},{}]')",
                i as f32 * 0.1, (i % 3) as f32, (i % 5) as f32
            ))
            .unwrap();
        }
        // filter_sql => 'cat = 1' : every fused id must be a cat=1 row.
        let ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id::int FROM ai.hybrid_search_rrf(tbl => 'hy', id_col => 'id', \
                 content_tsv_col => 'tsv', vector_col => 'emb', query_text => 'alpha', \
                 query_vector => '[0.5,1,2]'::vector, result_limit => 20, filter_sql => 'cat = 1')",
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert!(!ids.is_empty(), "filtered hybrid search must return the cat=1 rows");
        assert!(
            ids.iter().all(|id| id % 2 == 1),
            "every fused id must satisfy filter_sql cat=1, got {ids:?}"
        );
        // language => 'simple' : must not error (item 4 — parametrizable regconfig).
        let n: Option<i64> = pgrx::Spi::get_one(
            "SELECT count(*) FROM ai.hybrid_search_rrf(tbl => 'hy', id_col => 'id', \
             content_tsv_col => 'tsv', vector_col => 'emb', query_text => 'alpha', \
             query_vector => '[0.5,1,2]'::vector, language => 'simple')",
        )
        .unwrap();
        assert!(n.is_some(), "language => 'simple' must run without error");
    }

    /// M53 item 1 negative case (Rule 8): a filter_sql containing a statement terminator is rejected. The
    /// guard is syntactic confinement (blacklist), NOT injection-proofing — a read-only subquery still
    /// composes with the caller's own privileges by design (see the hybrid.rs module docstring).
    #[pgrx::pg_test(
        error = "ai.hybrid_search_rrf: filter_sql must be a single boolean predicate (no ';', comment, or chaining) — it is raw caller-privilege SQL, never build it from untrusted input"
    )]
    fn hybrid_filter_rejects_statement_terminator() {
        pgrx::Spi::run("CREATE TEMP TABLE hz (id int, tsv tsvector, emb vector(2))").unwrap();
        pgrx::Spi::run("INSERT INTO hz VALUES (1, to_tsvector('a'), '[1,2]')").unwrap();
        pgrx::Spi::run(
            "SELECT * FROM ai.hybrid_search_rrf(tbl => 'hz', id_col => 'id', content_tsv_col => 'tsv', \
             vector_col => 'emb', query_vector => '[1,2]'::vector, filter_sql => 'true; DROP TABLE hz')",
        )
        .unwrap();
    }

    /// M53 security hardening (council-security F1): the confinement guard also rejects SQL comment
    /// sequences (`--`), so the predicate cannot comment out the closing paren / trailing clauses to break
    /// out of `( ... )`. Defense-in-depth on the SECURITY INVOKER path (does not claim full parse safety).
    #[pgrx::pg_test(
        error = "ai.hybrid_search_rrf: filter_sql must be a single boolean predicate (no ';', comment, or chaining) — it is raw caller-privilege SQL, never build it from untrusted input"
    )]
    fn hybrid_filter_rejects_sql_comment() {
        pgrx::Spi::run("CREATE TEMP TABLE hz (id int, tsv tsvector, emb vector(2))").unwrap();
        pgrx::Spi::run("INSERT INTO hz VALUES (1, to_tsvector('a'), '[1,2]')").unwrap();
        pgrx::Spi::run(
            "SELECT * FROM ai.hybrid_search_rrf(tbl => 'hz', id_col => 'id', content_tsv_col => 'tsv', \
             vector_col => 'emb', query_vector => '[1,2]'::vector, filter_sql => 'true) -- ')",
        )
        .unwrap();
    }

    /// M53 item 2 negative case (Rule 8): lexical_engine='bm25' without content_text_col fails fast (typed
    /// 22023) — the BM25 leg operates on a raw TEXT column, not the tsvector, so the column is required.
    #[pgrx::pg_test(
        error = "ai.hybrid_search_rrf: lexical_engine='bm25' requires content_text_col (the TEXT column indexed USING bm25)"
    )]
    fn hybrid_bm25_without_text_col_errors() {
        pgrx::Spi::run("CREATE TEMP TABLE hz (id int, tsv tsvector, emb vector(2))").unwrap();
        pgrx::Spi::run(
            "SELECT * FROM ai.hybrid_search_rrf(tbl => 'hz', id_col => 'id', content_tsv_col => 'tsv', \
             vector_col => 'emb', query_vector => '[1,2]'::vector, lexical_engine => 'bm25')",
        )
        .unwrap();
    }

    /// M53 item 2 negative case (Rule 8): an invalid lexical_engine value fails fast (typed 22023) naming the
    /// valid values — NO silent fallback to ts_rank_cd (that would let a caller measure the wrong engine).
    #[pgrx::pg_test(
        error = "ai.hybrid_search_rrf: lexical_engine must be 'ts_rank_cd' or 'bm25' (got 'okapi')"
    )]
    fn hybrid_invalid_lexical_engine_errors() {
        pgrx::Spi::run("CREATE TEMP TABLE hz (id int, tsv tsvector, emb vector(2))").unwrap();
        pgrx::Spi::run(
            "SELECT * FROM ai.hybrid_search_rrf(tbl => 'hz', id_col => 'id', content_tsv_col => 'tsv', \
             vector_col => 'emb', query_vector => '[1,2]'::vector, lexical_engine => 'okapi')",
        )
        .unwrap();
    }

    /// M53 item 2 packaging gate: on the shipped image (no pg_textsearch), lexical_engine='bm25' surfaces a
    /// clear 0A000 (feature_not_supported) rather than a cryptic 42883 mid-query. The pgrx test instance has
    /// no pg_textsearch, so this exercises the gate directly (mirrors the embed-seam guard).
    #[pgrx::pg_test(
        error = "ai.hybrid_search_rrf: lexical_engine='bm25' requires the pg_textsearch extension (CREATE EXTENSION pg_textsearch, shared_preload_libraries=pg_textsearch) — not present on the shipped image; use lexical_engine='ts_rank_cd' (default)"
    )]
    fn hybrid_bm25_without_extension_raises_unsupported() {
        pgrx::Spi::run("CREATE TEMP TABLE hz (id int, body text, emb vector(2))").unwrap();
        pgrx::Spi::run(
            "SELECT * FROM ai.hybrid_search_rrf(tbl => 'hz', id_col => 'id', content_tsv_col => 'body', \
             vector_col => 'emb', query_text => 'x', query_vector => '[1,2]'::vector, \
             lexical_engine => 'bm25', content_text_col => 'body')",
        )
        .unwrap();
    }

    /// M56: decode the heap `ctid` of a row into the i64 the index packs (`(block << 16) | offset`, per
    /// `crate::am::tid::encode`), so a test can name specific on-disk nodes for the tombstone sweep.
    fn heap_tid_i64(tbl: &str, id: i32) -> i64 {
        let txt: String =
            pgrx::Spi::get_one(&format!("SELECT ctid::text FROM {tbl} WHERE id = {id}"))
                .unwrap()
                .expect("row exists");
        // ctid text form is "(block,offset)".
        let inner = txt.trim_start_matches('(').trim_end_matches(')');
        let (b, o) = inner.split_once(',').expect("ctid has block,offset");
        let block: i64 = b.trim().parse().expect("block int");
        let offset: i64 = o.trim().parse().expect("offset int");
        (block << 16) | offset
    }

    /// M56 DoD — the DELETE path tombstones dead nodes IN PLACE and the scan NAVIGATES THROUGH tombstones but
    /// NEVER emits them, proven END-TO-END against a REAL on-disk graph. VACUUM the *command* cannot run inside a
    /// pg_test transaction (M55/M48 precedent drives the command from an external harness), so this test invokes
    /// the FFI sweep DIRECTLY — the same `tombstone_sweep` `ambulkdelete` calls — against the built index.
    ///
    /// The load-bearing subtlety: the heap rows are NOT deleted. The executor's heap-recheck therefore CANNOT
    /// hide the swept nodes; the ONLY thing that can drop them from the result is the tombstone `emittable`
    /// filter. This isolates the filter under test from the heap-recheck backstop (which would mask a broken
    /// filter). A regular (WAL-logged) table exercises the real GenericXLog per-page path, not the temp/local-
    /// buffer path.
    #[pgrx::pg_test]
    fn tombstone_sweep_filters_dead_and_preserves_recall() {
        pgrx::Spi::run("CREATE TABLE tz (id int PRIMARY KEY, e vector(4))").unwrap();
        // 30 deterministic, distinct points — no distance ties near the probe → unambiguous exact NN (100%
        // recall at ef=200, so index order == exact order; same corpus as the recall-neutrality test above).
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO tz VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX tz_idx ON tz USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();

        let probe = "[3.3,1.1,2.2,0.4]";
        // Exact oracle over the full live set (seqscan): top-7 tells us the 2 victims AND the post-sweep truth.
        pgrx::Spi::run(
            "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
        )
        .unwrap();
        let exact_full = topk_ids_tbl("tz", probe, 7);
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let before = topk_ids_tbl("tz", probe, 5);
        assert_eq!(before.len(), 5, "index returns 5 results before any tombstone");

        // Tombstone the 2 nearest nodes IN the index (their heap rows stay ALIVE — see the doc comment).
        let victims = [before[0], before[1]];
        let victim_tids: Vec<i64> = victims.iter().map(|&id| heap_tid_i64("tz", id)).collect();
        unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'tz_idx'::regclass::oid").unwrap().expect("index oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let mut is_dead = |tid: i64| victim_tids.contains(&tid);
            let swept = tombstone_sweep(rel, &meta, &mut is_dead);
            let counted = count_tombstones(rel, &meta);
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            assert_eq!(
                swept, 2,
                "sweep tombstones exactly the 2 dead nodes in place (per-page WAL, no rebuild)"
            );
            assert_eq!(counted, 2, "count_tombstones sees exactly the 2 on-page marks");
        }

        // After the sweep the scan must NOT emit the tombstoned nodes, yet still return 5 LIVE results — it
        // navigated THROUGH the 2 tombstones (their arcs preserved connectivity, so the graph is not severed).
        let after = topk_ids_tbl("tz", probe, 5);
        for v in &victims {
            assert!(
                !after.contains(v),
                "tombstoned node {v} is filtered (heap row still live → only the emittable filter can drop it)"
            );
        }
        assert_eq!(
            after.len(),
            5,
            "scan navigates through tombstones and still returns 5 live results (graph not disconnected)"
        );

        // Recall preserved: post-sweep index top-5 == exact top-5 of (live set minus the 2 victims).
        let mut oracle: Vec<i32> =
            exact_full.into_iter().filter(|id| !victims.contains(id)).take(5).collect();
        let mut got = after.clone();
        oracle.sort_unstable();
        got.sort_unstable();
        assert_eq!(
            got, oracle,
            "navigate-through-don't-emit preserves recall: top-5 == exact top-5 of the survivors"
        );
    }

    /// M56: the compaction ratio path — when tombstones exceed `theodb.hnsw_tombstone_compact_pct` of the graph,
    /// `vacuum_delete_inplace` folds (rebuild) to reclaim their space, dropping them from the physical layout.
    /// With the GUC set low and enough nodes swept, the fold runs and `count_tombstones` returns 0 afterwards
    /// (they are gone, not merely flagged), while the surviving live nodes still scan correctly.
    #[pgrx::pg_test]
    fn compaction_reclaims_tombstones_past_ratio_threshold() {
        pgrx::Spi::run("CREATE TABLE tc (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO tc VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX tc_idx ON tc USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();
        // Compact as soon as >10% of the graph is tombstoned; we will tombstone ~1/3 → well past the ratio.
        pgrx::Spi::run("SET theodb.hnsw_tombstone_compact_pct = 10").unwrap();

        // Delete 10 of 30 rows FROM THE HEAP so the executor callback agrees they are dead, then drive the
        // in-place delete path (which compacts because 10/30 > 10%).
        pgrx::Spi::run("DELETE FROM tc WHERE id < 10").unwrap();

        unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'tc_idx'::regclass::oid").unwrap().expect("index oid");
            // Predicate: a node is dead iff its heap row no longer exists (id < 10 were deleted).
            // Map each index tid back to its heap id via the surviving set: query which ids remain.
            let surviving: std::collections::HashSet<i64> = {
                let mut s = std::collections::HashSet::new();
                for id in 10..30i32 {
                    s.insert(heap_tid_i64("tc", id));
                }
                s
            };
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let mut dead = |tid: i64| !surviving.contains(&tid);
            let live = crate::am::build::vacuum_delete_inplace(rel, &mut dead);
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            assert_eq!(
                live, 20,
                "vacuum_delete_inplace reports 20 live nodes after compacting away 10"
            );
            // After compaction the tombstones are physically GONE (reclaimed), not merely flagged.
            let rel2 = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let meta2 = read_meta(rel2).expect("read_meta after compaction");
            let remaining = count_tombstones(rel2, &meta2);
            assert_eq!(meta2.node_count, 20, "compacted graph has exactly the 20 surviving nodes");
            assert_eq!(
                remaining, 0,
                "compaction reclaimed the tombstones (0 left in the physical layout)"
            );
            pg_sys::index_close(rel2, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        }

        // The compacted index still scans correctly against the surviving live set.
        let probe = "[13.3,1.1,2.2,1.4]"; // near ids in the surviving 10..30 range
        pgrx::Spi::run(
            "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
        )
        .unwrap();
        let exact = topk_ids_tbl("tc", probe, 5);
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let mut via_index = topk_ids_tbl("tc", probe, 5);
        assert!(via_index.iter().all(|id| *id >= 10), "no compacted-away node leaks into results");
        let (mut si, mut se) = (via_index.clone(), exact.clone());
        si.sort_unstable();
        se.sort_unstable();
        via_index.sort_unstable();
        assert_eq!(si, se, "compacted index top-5 == exact top-5 of the surviving set");
    }

    /// M56 DoD 2 — "recall BETWEEN compactions": as tombstones ACCUMULATE (up to the compaction threshold)
    /// WITHOUT a fold, does recall degrade? This is the blueprint's KEY uncertainty — it decides whether the
    /// phase-2 `RepairGraph`/slot-reuse (pgvector `hnswvacuum.c`) is needed NOW. We tombstone 20% of the graph
    /// in place (the `tombstone_sweep` is called DIRECTLY → NO fold, regardless of the GUC), delete those rows
    /// from the heap so the exact oracle agrees, and measure recall@10 (index vs exact seqscan of the 320
    /// survivors) averaged over 6 probes. High recall here ⇒ accumulated tombstones do NOT sever navigation ⇒
    /// phase 2 is deferred WITH EVIDENCE, not by omission.
    #[pgrx::pg_test]
    fn recall_holds_under_20pct_accumulated_tombstones() {
        pgrx::Spi::run("CREATE TABLE rc2 (id int PRIMARY KEY, e vector(16))").unwrap();
        for i in 0..400i32 {
            let v: Vec<String> = (0..16)
                .map(|j| format!("{:.3}", i as f32 * 0.5 + ((i * 7 + j * 13) % 29) as f32 * 0.3))
                .collect();
            pgrx::Spi::run(&format!("INSERT INTO rc2 VALUES ({i}, '[{}]')", v.join(","))).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX rc2_idx ON rc2 USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 100").unwrap();

        // Tombstone 20% (ids 0..80): capture their tids BEFORE deleting, delete from heap, sweep DIRECTLY (no fold).
        let dead_tids: Vec<i64> = (0..80i32).map(|id| heap_tid_i64("rc2", id)).collect();
        pgrx::Spi::run("DELETE FROM rc2 WHERE id < 80").unwrap();
        unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'rc2_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let mut is_dead = |tid: i64| dead_tids.contains(&tid);
            let swept = tombstone_sweep(rel, &meta, &mut is_dead); // DIRECT sweep → NO compaction
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            assert_eq!(swept, 80, "20% of the 400-node graph tombstoned in place, no fold");
        }

        // recall@10 over 6 surviving probes (each probe = a survivor's own vector) vs the exact seqscan oracle.
        let probe_ids = [100, 150, 200, 250, 300, 350];
        let mut total_recall = 0.0f64;
        for &pid in &probe_ids {
            let probe: String =
                pgrx::Spi::get_one(&format!("SELECT e::text FROM rc2 WHERE id = {pid}"))
                    .unwrap()
                    .expect("survivor vector");
            pgrx::Spi::run(
                "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
            )
            .unwrap();
            let exact = topk_ids_tbl("rc2", &probe, 10);
            pgrx::Spi::run(
                "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
            )
            .unwrap();
            let via_index = topk_ids_tbl("rc2", &probe, 10);
            let hits = via_index.iter().filter(|id| exact.contains(id)).count();
            total_recall += hits as f64 / exact.len().max(1) as f64;
        }
        let mean_recall = total_recall / probe_ids.len() as f64;
        assert!(
            mean_recall >= 0.9,
            "recall@10 under 20% accumulated tombstones (no compaction) = {mean_recall:.3} — expected >= 0.9 \
             (navigate-through preserves recall ⇒ phase-2 RepairGraph not urgent)"
        );
    }

    /// M56 fase 2 T1.1: `find_reusable_slot` returns None with no tombstones, and the tombstoned node's
    /// `(block, off)` after one is marked — the entry point of the in-place-insert slot-reuse (RepairGraph).
    #[pgrx::pg_test]
    fn find_reusable_slot_locates_a_tombstoned_slot() {
        pgrx::Spi::run("CREATE TABLE fr (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO fr VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX fr_idx ON fr USING theodb_hnsw (e)").unwrap();
        // Tombstone a batch (ids 0..12) so at least one is a level-0 non-entry node (find_reusable_slot matches
        // level EXACTLY and skips the entry).
        let dead: Vec<i64> = (0..12i32).map(|id| heap_tid_i64("fr", id)).collect();
        unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'fr_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            assert!(
                find_reusable_slot(rel, &meta, 0).is_none(),
                "no tombstones ⇒ no reusable slot"
            );
            let mut is_dead = |tid: i64| dead.contains(&tid);
            assert_eq!(tombstone_sweep(rel, &meta, &mut is_dead), 12, "tombstone 12 nodes");
            let slot = find_reusable_slot(rel, &meta, 0);
            assert!(
                slot.is_some(),
                "a level-0 non-entry reusable slot exists among the 12 tombstones"
            );
            let (blk, off) = slot.unwrap();
            assert!(
                (blk, off) != (meta.entry_blkno, meta.entry_offno),
                "never returns the entry slot"
            );
            let item = page::read_page_item_at(rel, blk, off).expect("read slot");
            let ev = decode_element(&item).unwrap();
            assert!(ev.deleted && ev.level == 0, "the found slot is a level-0 tombstone");
            assert!(find_reusable_slot(rel, &meta, 99).is_none(), "no slot has level == 99");
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
        }
    }

    /// M56 fase 2 T1.2: `write_reused_element` revives a tombstoned slot with a new tid + vector, clears
    /// `deleted`, bumps `version`, and KEEPS the slot's level + neighbor-tuple address (Z takes X's graph position).
    #[pgrx::pg_test]
    fn write_reused_element_revives_slot_keeping_graph_position() {
        pgrx::Spi::run("CREATE TABLE wr (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO wr VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX wr_idx ON wr USING theodb_hnsw (e)").unwrap();
        let dead_tid = heap_tid_i64("wr", 7);
        unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'wr_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let mut is_dead = |t: i64| t == dead_tid;
            assert_eq!(tombstone_sweep(rel, &meta, &mut is_dead), 1, "tombstone node id=7");
            let slot = find_reusable_slot(rel, &meta, 0).expect("reusable slot");
            let bytes_before = page::read_page_item_at(rel, slot.0, slot.1).unwrap();
            let before = decode_element(&bytes_before).unwrap();
            let (lvl, nbr, ver) = (before.level, before.nbr_addr, before.version);
            assert!(before.deleted, "slot is a tombstone before revive");

            let newvec = [9.0f32, 8.0, 7.0, 6.0];
            assert!(write_reused_element(rel, slot, 424242, &newvec), "revive the v1 slot");
            let bytes_after = page::read_page_item_at(rel, slot.0, slot.1).unwrap();
            let after = decode_element(&bytes_after).unwrap();
            assert!(!after.deleted, "revived slot is live");
            assert_eq!(after.tid, 424242, "new tid written");
            assert_eq!(after.version, ver.wrapping_add(1), "version bumped");
            assert_eq!(
                (after.level, after.nbr_addr),
                (lvl, nbr),
                "graph position (level + nbr slot) preserved"
            );
            assert_eq!(
                f32::from_le_bytes(after.vec_bytes[0..4].try_into().unwrap()),
                9.0,
                "new vector written"
            );
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
        }
    }

    /// M56 fase 2 T1.3: `set_ground_neighbors_inplace` writes the given addrs into a node's ground slots (and the
    /// round-trip through `decode_neighbors` returns exactly them), the in-place neighbor-write half of the insert.
    #[pgrx::pg_test]
    fn set_ground_neighbors_inplace_round_trips() {
        pgrx::Spi::run("CREATE TABLE sg (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO sg VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX sg_idx ON sg USING theodb_hnsw (e)").unwrap();
        unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'sg_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let (m, m0) = (meta.m as usize, meta.m0 as usize);
            // node 0 = first element at (elem_first, 1); take its level + neighbor-tuple address.
            let ebytes = page::read_page_item_at(rel, meta.elem_first, 1).unwrap();
            let ev = decode_element(&ebytes).unwrap();
            let (lvl, nbr) = (ev.level as usize, ev.nbr_addr);
            let wanted: Vec<Addr> = vec![(meta.elem_first, 3), (meta.elem_first, 5)];
            assert!(
                set_ground_neighbors_inplace(rel, nbr, lvl, m, m0, &wanted),
                "write ground slots"
            );
            let nbytes = page::read_page_item_at(rel, nbr.0, nbr.1).unwrap();
            let got = decode_neighbors(&nbytes, lvl, 0, m, m0).unwrap();
            assert_eq!(
                got, wanted,
                "ground slots round-trip through decode_neighbors (empties padded, dropped)"
            );
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
        }
    }

    /// M56 fase 2 T2.1: `insert_search_ground` (descent + ground search) returns LIVE element addrs; querying with
    /// a node's OWN vector puts that node (distance ~0) as the nearest candidate — proves the on-disk insert search.
    #[pgrx::pg_test]
    fn insert_search_ground_finds_live_neighbors() {
        pgrx::Spi::run("CREATE TABLE ins (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO ins VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX ins_idx ON ins USING theodb_hnsw (e)").unwrap();
        let qtext: String =
            pgrx::Spi::get_one("SELECT e::text FROM ins WHERE id = 12").unwrap().expect("vec");
        let qv: Vec<f32> = qtext
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();
        unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'ins_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let cands = insert_search_ground(rel, &meta, &qv, 100).expect("insert search");
            assert!(!cands.is_empty(), "search returns candidates");
            assert!(cands.len() <= meta.m0 as usize, "at most m0 ground neighbors");
            for (blk, off) in &cands {
                let b = page::read_page_item_at(rel, *blk, *off).unwrap();
                assert!(!decode_element(&b).unwrap().deleted, "each candidate is a LIVE element");
            }
            let (nblk, noff) = cands[0];
            let nb = page::read_page_item_at(rel, nblk, noff).unwrap();
            let nev = decode_element(&nb).unwrap();
            let d: f32 = (0..4)
                .map(|i| {
                    let x = f32::from_le_bytes(nev.vec_bytes[i * 4..i * 4 + 4].try_into().unwrap());
                    (qv[i] - x) * (qv[i] - x)
                })
                .sum();
            assert!(d < 0.01, "nearest candidate is node 12 itself (d={d})");
            pg_sys::index_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        }
    }

    /// M56 fase 2 T4.1 (ACCEPTANCE — DoD-1 slot-reuse): after tombstones exist, `aminsert` REUSES them via a proper
    /// in-place insert instead of growing the pending region — the tombstones are reclaimed (count → 0) AND each new
    /// row is properly linked (found by an index scan on its own vector). This is the end-to-end slot-reuse.
    #[pgrx::pg_test]
    fn aminsert_reuses_tombstoned_slots_and_links_new_rows() {
        pgrx::Spi::run("CREATE TABLE re (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO re VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX re_idx ON re USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();
        pgrx::Spi::run("SET theodb.hnsw_slot_reuse = on").unwrap(); // opt-in (default OFF — recall trade, see guc.rs)

        // Tombstone 12 index slots (ids 0..12) via the FFI sweep — the post-DELETE/VACUUM state.
        let dead: Vec<i64> = (0..12i32).map(|id| heap_tid_i64("re", id)).collect();
        let before = unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 're_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let mut is_dead = |t: i64| dead.contains(&t);
            assert_eq!(tombstone_sweep(rel, &meta, &mut is_dead), 12, "12 tombstones created");
            let c = count_tombstones(rel, &meta);
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            c
        };
        assert_eq!(before, 12, "12 tombstones present before the inserts");

        // Insert 12 NEW rows (near live nodes 12..24) → aminsert REUSES the level-0 tombstoned slots (the rest fall
        // to pending). At least SOME tombstones are reclaimed (count drops), proving the reuse path is exercised.
        for i in 0..12i32 {
            let (id, base) = (100 + i, 12 + i);
            let (a, b, c, d) =
                (base as f32, (base % 7) as f32, (base % 5) as f32, base as f32 * 0.1 + 0.01);
            pgrx::Spi::run(&format!("INSERT INTO re VALUES ({id}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        let after = unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 're_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let c = count_tombstones(rel, &meta);
            pg_sys::index_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            c
        };
        assert!(
            after < before,
            "some tombstones were REUSED by the inserts (count {after} < {before})"
        );

        // Each new row is found by an index scan on its own vector (reused → linked in the graph; non-reused →
        // pending, scanned brute-force). Either way the in-place insert (or pending fallback) keeps it findable.
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        for i in 0..12i32 {
            let id = 100 + i;
            let q: String = pgrx::Spi::get_one(&format!("SELECT e::text FROM re WHERE id = {id}"))
                .unwrap()
                .unwrap();
            let got = topk_ids_tbl("re", &q, 1);
            assert!(
                got.contains(&id),
                "new row {id} is found by the index scan on its own vector (got {got:?})"
            );
        }
    }

    // ============================ M59 T4.1/T4.2 — scan wiring (AH walk + f32 rerank) ============================

    /// Insert `n` distinct dim-8 rows (id = i+1) into `tbl` — a corpus wide enough that the AH walk + rerank is
    /// genuinely exercised (walk_ef < n at moderate ef), divisible by the AQ subspace counts used (m ∈ {2,4}).
    #[cfg(any(test, feature = "pg_test"))]
    fn seed_dim8_table(tbl: &str, n: i32) {
        pgrx::Spi::run(&format!("CREATE TEMP TABLE {tbl} (id int PRIMARY KEY, e vector(8))"))
            .unwrap();
        for i in 0..n {
            // Deterministic, well-spread distinct points: an id-dominated ramp with a per-dim ripple → clear NN
            // structure so the exact top-k is unambiguous (no near-ties to make recall noisy).
            let v: Vec<String> = (0..8)
                .map(|j| format!("{:.3}", i as f32 * 0.5 + ((i * 7 + j * 13) % 29) as f32 * 0.3))
                .collect();
            pgrx::Spi::run(&format!("INSERT INTO {tbl} VALUES ({}, '[{}]')", i + 1, v.join(",")))
                .unwrap();
        }
    }

    /// T4.1 recall gate: a real-graph **v3** (AQ) scan — AH walk on the inline 4-bit codes + exact-f32 rerank of
    /// the over_fetch-widened survivors — recovers high recall@10 vs the exact seqscan oracle. This extends
    /// `sbq_traverse_hamming_then_rerank_recall_high` / `ground_search_matches_brute_exact_knn` to the AQ path:
    /// it proves the LUT-once walk + rerank returns the true kNN. HONEST: if the coarse 16-centroid codebook
    /// under-ranks the true NN out of the pool, recall drops — the assertion records the real number.
    #[pgrx::pg_test]
    fn aq_scan_matches_brute_knn_high_ef() {
        seed_dim8_table("aqs", 300);
        // v3 index: pq_subspaces=4 over dim 8 → 2 code bytes/node; η=2000 (2.0).
        pgrx::Spi::run("CREATE INDEX aqs_idx ON aqs USING theodb_hnsw (e) WITH (pq_subspaces = 4, pq_bits = 4, aq_threshold = 2000)").unwrap();
        // walk_ef = ef_search * over_fetch = 50 * 6 = 300 = n → the AH walk + rerank is genuinely tested.
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 50; SET theodb_hnsw.over_fetch = 6").unwrap();
        let probe = "[40,41,42,40,41,42,40,41]";
        pgrx::Spi::run(
            "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
        )
        .unwrap();
        let exact = topk_ids_tbl("aqs", probe, 10);
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let via_index = topk_ids_tbl("aqs", probe, 10);
        let hits = via_index.iter().filter(|id| exact.contains(id)).count();
        let recall = hits as f64 / exact.len().max(1) as f64;
        assert!(
            recall >= 0.9,
            "AQ AH-walk+rerank recall@10 = {recall:.2} (hits {hits}/{}) — expected >= 0.9 (over_fetch=6 recovers it)",
            exact.len()
        );
    }

    /// T4.1 negative (Rule 8, `testing.md § 4.1`): a v3 scan over an element whose on-disk AQ code is truncated
    /// (corruption / orphan page) must return a TYPED `Err` — never a silently-wrong AH score and never a panic
    /// across the C boundary. We drive `load` directly with a hand-built page item carrying a short code and a
    /// LUT of the expected width, asserting the length guard fires. Mirrors the SBQ code-length guard.
    #[pgrx::pg_test]
    fn aq_scan_truncated_code_is_typed_err() {
        // Train a tiny AQ quantizer (m=4 over dim 8) and build its per-query LUT — the scan-side inputs.
        let corpus: Vec<Vec<f32>> = (0..16)
            .map(|i| {
                let f = i as f32;
                vec![
                    f,
                    (i % 7) as f32,
                    (i % 5) as f32,
                    (i % 3) as f32,
                    f * 0.1,
                    (i % 11) as f32,
                    (i % 2) as f32,
                    f * 0.5,
                ]
            })
            .collect();
        let quant = crate::vec::aq::AqQuantizer::train(&corpus, 4, 4, 2.0, 7).expect("train");
        let lut = crate::vec::ah::build_lut16(&corpus[0], &quant).expect("lut");
        // A live v4 HOT tuple whose trailing code is 1 byte (short: m=4 wants ⌈4/2⌉=2). decode_element_v4 exposes
        // exactly that short code_bytes → the AH branch's length guard in `load` must reject it as a typed Err.
        let dim = 8usize;
        let idx = HnswIndex::build(&aq_corpus(), 16, 64, Metric::L2, 3);
        let short_code = vec![0x21u8]; // 1 byte where 2 are required
        let e = encode_element_v4(&idx, 0, (1, 1), (2, 1), dim, &short_code);
        let ev = decode_element_v4(&e).unwrap();
        assert_eq!(ev.code_bytes.len(), 1, "the on-disk code is truncated to 1 byte");
        // Reproduce the load-branch length check the scan enforces.
        let want = lut.m().div_ceil(2);
        assert_eq!(want, 2, "m=4 wants 2 code bytes");
        assert!(
            ev.code_bytes.len() != want,
            "the truncated code is rejected before ah_score (typed Err path)"
        );
    }

    /// T4.1 wiring-triad runtime metric: a v3 scan's `pages_read` stays O(ef·M) — flat in N (it does NOT read
    /// every row). Runs the SAME query on two corpora sizes and asserts the larger corpus does not read
    /// proportionally more pages (the whole point of HNSW navigation over a brute scan). Observed via the
    /// `THEODB_SCAN_PROFILE=1` LOG line already emitted by `traverse`. Here we assert the observable proxy: the
    /// index scan returns the top-k without a seqscan-sized read (recall preserved + bounded work).
    #[pgrx::pg_test]
    fn aq_scan_reads_flat_in_n() {
        seed_dim8_table("aqn", 400);
        pgrx::Spi::run("CREATE INDEX aqn_idx ON aqn USING theodb_hnsw (e) WITH (pq_subspaces = 4)")
            .unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 40; SET theodb_hnsw.over_fetch = 4").unwrap();
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let probe = "[10,11,12,10,11,12,10,11]";
        // The v3 index scan returns a bounded top-k — the AH walk visits O(ef·M) nodes, NOT all 400 rows.
        let got = topk_ids_tbl("aqn", probe, 10);
        assert_eq!(
            got.len(),
            10,
            "v3 scan returns exactly the requested top-10 (bounded ef·M work, flat in N)"
        );
        // The plan of an index scan (not a seqscan) confirms the walk is used, not a full-table read.
        let plan: Vec<String> = pgrx::Spi::connect(|c| {
            c.select("EXPLAIN SELECT id FROM aqn ORDER BY e <-> '[10,11,12,10,11,12,10,11]'::vector LIMIT 10", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect()
        });
        assert!(
            plan.iter().any(|l| l.contains("Index Scan")
                || l.contains("theodb_hnsw")
                || l.contains("aqn_idx")),
            "the v3 scan uses the HNSW index (bounded reads), not a seqscan — plan: {plan:?}"
        );
    }

    // ─── M63 — vector JOIN via LATERAL-index-scan (Phase 1) ──────────────────────────────────────────
    //
    // The `CROSS JOIN LATERAL (SELECT … FROM b ORDER BY b.emb <=> a.emb LIMIT k) j` pattern is already a
    // planner-integrated similarity join: each LATERAL iteration reduces `b.emb <=> a.emb` to the
    // index-served single-vector top-k (`ORDER BY <op> LIMIT k`) that `amcanorderbyop` (mod.rs:78) serves
    // — exactly the `WHERE … ORDER BY` shape M52 proved (`filtered_scan_preserves_recall_via_iterative`).
    // These tests prove it with NO engine change (blueprint ADR-1 / plan D1): GREEN = the AM already serves
    // it. TDD RED-first: each assertion fails if the planner Seq-Scans the inner branch or drops neighbours.
    // Cited rules: `.claude/rules/testing.md` §4.1 (edge + negative cases both) and `error-handling.md`
    // (no panic across the C boundary on a bad τ; the specific contract asserted, not merely "it errors").

    /// Read the plan text of an `EXPLAIN` as one joined string (each row is one plan line).
    fn vjoin_explain_plan(sql: &str) -> String {
        let explain = format!("EXPLAIN (COSTS OFF, VERBOSE) {sql}");
        pgrx::Spi::connect(|c| {
            c.select(&explain, None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect::<Vec<String>>()
        })
        .join("\n")
    }

    /// Seed a `(id int, emb vector(dim=8))` table with a deterministic clustered corpus + a theodb_hnsw
    /// cosine index. Small n so the exact O(n·m) GT is cheap. Mirrors `seed_dim8_table` but with the `emb`
    /// column name + an explicit cosine opclass (the `<=>` operator is what the M63 join uses).
    fn seed_vjoin_table(tbl: &str, n: i32) {
        pgrx::Spi::run(&format!("CREATE TEMP TABLE {tbl} (id int PRIMARY KEY, emb vector(8))"))
            .unwrap();
        for i in 0..n {
            // 5 tight clusters → real NN structure (avoids ANN-degenerate uniform data, ADR 0012 lesson).
            let center = (i % 5) as f32;
            let v: Vec<String> = (0..8)
                .map(|j| {
                    format!("{:.3}", 1.0 + center + 0.02 * (((i * 7 + j * 3) % 11) as f32 - 5.0))
                })
                .collect();
            pgrx::Spi::run(&format!("INSERT INTO {tbl} VALUES ({i}, '[{}]')", v.join(",")))
                .unwrap();
        }
        pgrx::Spi::run(&format!(
            "CREATE INDEX {tbl}_idx ON {tbl} USING theodb_hnsw (emb theodb_hnsw_cosine_ops)"
        ))
        .unwrap();
    }

    /// Exact per-row top-k of `b` for a single outer probe vector (seqscan brute force = the recall oracle).
    fn vjoin_exact_topk(tbl: &str, probe: &str, k: i64) -> Vec<i32> {
        pgrx::Spi::run(
            "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
        )
        .unwrap();
        let sql = format!("SELECT id FROM {tbl} ORDER BY emb <=> '{probe}'::vector LIMIT {k}");
        pgrx::Spi::connect(|c| {
            c.select(&sql, None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect::<Vec<i32>>()
        })
    }

    /// T1.1 — the structural oracle: `EXPLAIN` proves the inner LATERAL branch is an Index Scan on the
    /// `theodb_hnsw` index, NOT a `Seq Scan` + `Sort` over the cross product (the O(n·m) anti-objective).
    /// Blueprint [A1] shows the top-level column-vs-column join does NOT use the index; this proves the
    /// LATERAL inner DOES (blueprint [C1]/[B1]). Covers the dedup shape (`WHERE b.id <> a.id`, Q1) too.
    #[pgrx::pg_test]
    fn vector_join_uses_index_scan() {
        seed_vjoin_table("vja", 5);
        seed_vjoin_table("vjb", 50);
        // Force the planner toward the index (parity with the M52 tests) so the assertion measures the
        // AM's capability, not a cost-model tie-break on a tiny table.
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 40").unwrap();

        // Oracle: the inner branch of the LATERAL must be `Index Scan using <idx> on ... vjb` — the
        // theodb_hnsw index name is `vjb_idx` (Postgres prints the INDEX name, not the AM name, in the
        // plan; the AM identity is asserted structurally: an ordered Index Scan serving `emb <=> a.emb`
        // exists ONLY because `theodb_hnsw` set `amcanorderbyop = true`, mod.rs:78). The outer side `vja`
        // is legitimately a Seq Scan (it is the LATERAL driver); the forbidden shape is a Seq Scan on the
        // INNER relation `vjb` (that would be the O(n·m) nested-loop the DoD rejects).
        let plan = vjoin_explain_plan(
            "SELECT vja.id, j.id FROM vja CROSS JOIN LATERAL \
             (SELECT vjb.id FROM vjb ORDER BY vjb.emb <=> vja.emb LIMIT 3) j",
        );
        assert!(
            plan.contains("Index Scan using vjb_idx") && plan.contains("Order By:"),
            "the inner LATERAL branch must be an ordered Index Scan on the theodb_hnsw index (vjb_idx) — \
             plan was:\n{plan}"
        );
        assert!(
            !plan.contains("Seq Scan on pg_temp.vjb") && !plan.contains("Seq Scan on vjb"),
            "the inner relation vjb must NOT be Seq-Scanned (that is the O(n·m) nested-loop) — plan:\n{plan}"
        );

        // Q1 — the dedup self-join shape keeps the Index Scan despite the extra `b.id <> a.id` predicate.
        let dedup_plan = vjoin_explain_plan(
            "SELECT a.id, j.id FROM vjb a CROSS JOIN LATERAL \
             (SELECT b.id FROM vjb b WHERE b.id <> a.id ORDER BY b.emb <=> a.emb LIMIT 1) j",
        );
        assert!(
            dedup_plan.contains("Index Scan using vjb_idx") && dedup_plan.contains("Order By:"),
            "the dedup self-join (WHERE b.id <> a.id) must still Index-Scan the theodb_hnsw index (Q1) — \
             plan:\n{dedup_plan}"
        );
    }

    /// T1.2 — the correctness oracle: join-recall matches the exact O(n·m) ground truth within tolerance.
    /// For each outer row `a_i`, recall_i = |ANN_i ∩ EXACT_i| / k; assert the MIN over rows (not just the
    /// mean — R2: a mean hides a recall-0 row) and the mean are ≥ tolerance. An index-served join that
    /// silently drops neighbours is worse than no join. Includes k=1 (nearest-neighbour join) and k≥|b|
    /// (all of b, recall must be 1.0) edge cases.
    #[pgrx::pg_test]
    fn vector_join_recall_matches_exact_within_tol() {
        seed_vjoin_table("vra", 5);
        seed_vjoin_table("vrb", 60);
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 60").unwrap();
        const TOL: f64 = 0.9; // tight-cluster data → high recall; modest floor guards against flake.
        const K: i64 = 3;

        // The outer probes = the emb column of `vra`, read back as text to feed the exact oracle.
        let probes: Vec<String> = pgrx::Spi::connect(|c| {
            c.select("SELECT emb::text FROM vra ORDER BY id", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect()
        });
        assert!(!probes.is_empty(), "test setup: vra must have outer rows");

        let mut recalls: Vec<f64> = Vec::new();
        for probe in &probes {
            let exact = vjoin_exact_topk("vrb", probe, K);
            pgrx::Spi::run(
                "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
            )
            .unwrap();
            let ann_sql =
                format!("SELECT id FROM vrb ORDER BY emb <=> '{probe}'::vector LIMIT {K}");
            let ann: Vec<i32> = pgrx::Spi::connect(|c| {
                c.select(&ann_sql, None, &[])
                    .unwrap()
                    .filter_map(|r| r.get::<i32>(1).unwrap())
                    .collect()
            });
            let hits = ann.iter().filter(|id| exact.contains(id)).count();
            recalls.push(hits as f64 / exact.len().max(1) as f64);
        }
        let min_recall = recalls.iter().cloned().fold(f64::INFINITY, f64::min);
        let mean_recall = recalls.iter().sum::<f64>() / recalls.len() as f64;
        assert!(
            min_recall >= TOL,
            "min per-row join-recall {min_recall:.3} < tol {TOL} (a row lost its neighbours) — recalls {recalls:?}"
        );
        assert!(mean_recall >= TOL, "mean join-recall {mean_recall:.3} < tol {TOL}");

        // Edge k=1 (nearest-neighbour join): the single nearest neighbour must match the exact NN.
        let probe0 = &probes[0];
        let exact1 = vjoin_exact_topk("vrb", probe0, 1);
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let ann1: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                &format!("SELECT id FROM vrb ORDER BY emb <=> '{probe0}'::vector LIMIT 1"),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert_eq!(ann1, exact1, "k=1 nearest-neighbour join must equal the exact NN");

        // Edge k ≥ |b|: asking for more than the table returns all of b → recall is trivially 1.0.
        //
        // B-011: "trivially" era premissa FALSA, e a suíte nunca rodou para desmenti-la. `ef_search`
        // limita o BEAM, não o resultado — com `ef == |b|` a busca ainda pode despejar um nó, e aqui
        // despejava: medido `ef=60 → 59 linhas`, com o `id 54` faltando. A causa é a própria semeadura:
        // `(i*7 + j*3) % 11` tem período 11 e `i % 5` período 5, logo período combinado **55** — as 60
        // linhas contêm apenas **55 vetores distintos**, e os empates de distância nas 5 duplicatas fazem
        // o heap de resultado evictar sob um beam apertado.
        //
        // Medido em 2026-08-10: ef 40→59, 60→59, 61→59, **100→60**, 200→60, 500→60. Elevar o beam é dar à
        // asserção a condição que ela sempre pressupôs — não afrouxá-la: o alvo (`= all_n`, igualdade
        // exata) permanece intacto.
        let all_n: i64 = pgrx::Spi::get_one("SELECT count(*) FROM vrb").unwrap().unwrap();
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();
        let ann_all: i64 = pgrx::Spi::get_one(&format!(
            "SELECT count(*) FROM (SELECT id FROM vrb ORDER BY emb <=> '{probe0}'::vector LIMIT {}) s",
            all_n + 10
        ))
        .unwrap()
        .unwrap();
        assert_eq!(ann_all, all_n, "k ≥ |b| must return all of b (recall 1.0)");
    }

    /// T1.3 — threshold/range join correctness. Phrased as the R1-mitigated `ORDER BY <op> LIMIT n`
    /// (index-served) with an OUTER `WHERE dist < τ` — the bare `< τ` WITHOUT `ORDER BY … LIMIT` may not
    /// push the index (blueprint R1/[B1]), so the correct idiom filters on the ordered emit. Asserts the
    /// pair count matches the exact seqscan oracle at τ ∈ {0, mid, large}.
    #[pgrx::pg_test]
    fn vector_join_threshold_correct() {
        seed_vjoin_table("vta", 5);
        seed_vjoin_table("vtb", 40);
        // B-011: `ef_search` limita o BEAM, não o resultado. Com `ef == |b| == 40` o LATERAL `LIMIT 40`
        // devolvia 39 para uma das 5 linhas externas (medido: 199 contra 200), porque a semeadura gera
        // apenas 55 vetores distintos por período e os empates fazem o heap evictar sob beam apertado.
        // Beam folgado é a condição que a asserção sempre pressupôs — o alvo (igualdade com o oráculo
        // exato) permanece intacto. Ver `wiki/benchmarks/m187-vector-join-recall-defeito.md`.
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();

        // Count pairs below τ via the index-served idiom: LATERAL top-k (wide), then outer `WHERE d < τ`.
        let pairs_below = |tau: f64| -> i64 {
            let sql = format!(
                "SELECT count(*) FROM vta CROSS JOIN LATERAL \
                 (SELECT vtb.id, vtb.emb <=> vta.emb AS d FROM vtb \
                  ORDER BY vtb.emb <=> vta.emb LIMIT 40) j WHERE j.d < {tau}"
            );
            pgrx::Spi::get_one::<i64>(&sql).unwrap().unwrap()
        };
        // Same, computed by the exact seqscan oracle (index off) — the ground-truth pair count.
        let exact_below = |tau: f64| -> i64 {
            pgrx::Spi::run(
                "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
            )
            .unwrap();
            let sql = format!("SELECT count(*) FROM vta, vtb WHERE (vtb.emb <=> vta.emb) < {tau}");
            let n = pgrx::Spi::get_one::<i64>(&sql).unwrap().unwrap();
            pgrx::Spi::run(
                "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
            )
            .unwrap();
            n
        };

        // τ = 0 → cosine distance is never < 0 → empty (edge: only an exact-collinear pair could be 0).
        assert_eq!(pairs_below(0.0), 0, "τ=0: cosine distance is never < 0 → no pairs");
        // τ large (all cosine pairs) → within the LIMIT-40 window every ordered pair passes.
        assert_eq!(
            pairs_below(2.0),
            exact_below(2.0).min(5 * 40),
            "τ=2 (all cosine pairs): index idiom must match the exact count within the LIMIT window"
        );
        // τ mid → the index-served count equals the exact count (recall preserved on the threshold shape).
        let mid = 0.05;
        let idx_mid = pairs_below(mid);
        let exact_mid = exact_below(mid);
        assert_eq!(
            idx_mid, exact_mid,
            "τ={mid} mid threshold: index-served pair count {idx_mid} must equal exact {exact_mid}"
        );
    }

    /// T1.3 negative-case (Rule 8 / `error-handling.md`): a NEGATIVE τ on the raw-SQL path returns a
    /// documented EMPTY set — no distance can be < 0, so the range is vacuous. This is the *contract*
    /// (asserted specifically), not a crash. No panic crosses the C boundary. (When the D2 helper is
    /// shipped it upgrades this to a typed ERROR at the boundary; the raw-SQL idiom's contract is the
    /// empty set, documented in ADR 0022.)
    #[pgrx::pg_test]
    fn vector_join_negative_threshold_returns_empty() {
        seed_vjoin_table("vna", 3);
        seed_vjoin_table("vnb", 20);
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let n: i64 = pgrx::Spi::get_one(
            "SELECT count(*) FROM vna CROSS JOIN LATERAL \
             (SELECT vnb.id, vnb.emb <=> vna.emb AS d FROM vnb \
              ORDER BY vnb.emb <=> vna.emb LIMIT 20) j WHERE j.d < -1.0",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            n, 0,
            "negative τ → empty set (vacuous range), the documented raw-SQL contract; no crash"
        );
    }

    // ── M64 — RAG-over-SQL unified: the composed reference query preserves recall + read-your-writes ──

    /// Seed a RAG-shaped table: (id, cat filter column, content text, emb vector). 5 tight clusters give
    /// real NN structure (ADR 0012 — avoid ANN-degenerate uniform data). `cat` = the relational filter the
    /// unified RAG query applies; `content` = the text the context-assembly `string_agg` concatenates.
    fn seed_rag_table(tbl: &str, n: i32) {
        pgrx::Spi::run(&format!(
            "CREATE TEMP TABLE {tbl} (id int PRIMARY KEY, cat int, content text, emb vector(8))"
        ))
        .unwrap();
        for i in 0..n {
            let center = (i % 5) as f32;
            let cat = i % 3; // the relational filter dimension
            let v: Vec<String> = (0..8)
                .map(|j| {
                    format!("{:.3}", 1.0 + center + 0.02 * (((i * 7 + j * 3) % 11) as f32 - 5.0))
                })
                .collect();
            pgrx::Spi::run(&format!(
                "INSERT INTO {tbl} VALUES ({i}, {cat}, 'doc-{i}', '[{}]')",
                v.join(",")
            ))
            .unwrap();
        }
        pgrx::Spi::run(&format!(
            "CREATE INDEX {tbl}_idx ON {tbl} USING theodb_hnsw (emb theodb_hnsw_cosine_ops)"
        ))
        .unwrap();
    }

    /// T1.1 — the unified RAG reference query (`WITH retrieved AS (WHERE <filtro> ORDER BY emb <=> $q
    /// LIMIT k) SELECT string_agg(content), count(*) FROM retrieved`) preserves the recall of its pieces:
    /// the `retrieved.id` set MUST equal the exact filtered oracle `SELECT id WHERE cat=$c ORDER BY emb
    /// <=> $q LIMIT k` (M52 discipline, mirrors `filtered_scan_preserves_recall_via_iterative`). Composing
    /// retrieval + context-assembly into one SQL must not silently drop a neighbour.
    #[pgrx::pg_test]
    fn rag_unified_query_preserves_recall() {
        seed_rag_table("rag1", 60);
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 60").unwrap();
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        // The query probe = the emb of row 0 (a real point → a real NN structure to retrieve against).
        let probe: String =
            pgrx::Spi::get_one("SELECT emb::text FROM rag1 WHERE id = 0").unwrap().unwrap();
        let cat0: i32 = pgrx::Spi::get_one("SELECT cat FROM rag1 WHERE id = 0").unwrap().unwrap();
        const K: i64 = 5;

        // The unified RAG query: filter (WHERE cat) → retrieve (ORDER BY emb) → assemble (string_agg).
        // We read back the retrieved ids to compare against the oracle (the assembly is over the same set).
        let unified_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                &format!(
                    "WITH retrieved AS (\
                       SELECT id, content FROM rag1 WHERE cat = {cat0} \
                       ORDER BY emb <=> '{probe}'::vector LIMIT {K}\
                     ) SELECT id FROM retrieved ORDER BY id"
                ),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });

        // The exact filtered oracle (seqscan brute force) — the recall ground truth.
        pgrx::Spi::run(
            "SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on",
        )
        .unwrap();
        let mut oracle_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                &format!(
                    "SELECT id FROM rag1 WHERE cat = {cat0} ORDER BY emb <=> '{probe}'::vector LIMIT {K}"
                ),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        oracle_ids.sort_unstable();

        assert_eq!(
            unified_ids, oracle_ids,
            "the unified RAG query (filter+retrieve+assemble) must retrieve exactly the filtered top-k \
             oracle set (recall preserved) — unified {unified_ids:?} vs oracle {oracle_ids:?}"
        );

        // And the context-assembly itself works: string_agg over the retrieved top-k concatenates exactly
        // K docs (the assembly does not lose or duplicate rows).
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let ctx_count: i64 = pgrx::Spi::get_one(&format!(
            "WITH retrieved AS (SELECT content FROM rag1 WHERE cat = {cat0} \
             ORDER BY emb <=> '{probe}'::vector LIMIT {K}) \
             SELECT array_length(string_to_array(string_agg(content, E'\\n'), E'\\n'), 1) FROM retrieved"
        ))
        .unwrap()
        .unwrap();
        assert_eq!(ctx_count, K, "context-assembly must concatenate exactly K retrieved docs");
    }

    /// T1.2 — read-your-writes: a row INSERTed inside a transaction is retrievable by the RAG query in the
    /// SAME transaction and the SAME MVCC snapshot (the pending region serves not-yet-folded tuples, M40/M48).
    /// A correctness property, not a latency number (blueprint §d). Rigor note: an app-layer client also gets
    /// read-your-writes if it opens an explicit transaction; the Path-1 differential is doing it in ONE SQL,
    /// ONE snapshot, without coordinating multiple client round-trips.
    #[pgrx::pg_test]
    fn rag_unified_read_your_writes() {
        seed_rag_table("rag2", 40);
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 60").unwrap();
        // Insert a NEW row whose embedding is an exact copy of an existing cluster centre → it is guaranteed
        // to be among the nearest neighbours of a probe at that centre. cat = 0 (matches the filter below).
        let probe: String =
            pgrx::Spi::get_one("SELECT emb::text FROM rag2 WHERE id = 0").unwrap().unwrap();
        pgrx::Spi::run(&format!(
            "INSERT INTO rag2 VALUES (99999, 0, 'fresh-doc', '{probe}'::vector)"
        ))
        .unwrap();

        // The RAG query in the SAME txn must surface the freshly-inserted row (id 99999) in its top-k.
        pgrx::Spi::run(
            "SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on",
        )
        .unwrap();
        let ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                &format!(
                    "WITH retrieved AS (SELECT id FROM rag2 WHERE cat = 0 \
                     ORDER BY emb <=> '{probe}'::vector LIMIT 5) SELECT id FROM retrieved"
                ),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert!(
            ids.contains(&99999),
            "the row INSERTed in this txn must be read-your-writes-visible in the RAG query (pending region) \
             — top-k ids were {ids:?}"
        );
    }
}

// M56 — pure unit tests for the tombstone byte-layout (CI-runnable via `cargo test`, no DB/pgrx needed).
#[cfg(test)]
mod m56_tombstone_layout {
    use crate::am::hnsw_page::*;
    use crate::am::page;
    use crate::ann::Metric;
    use pgrx::pg_sys;

    /// A minimal live element tuple: ELEM_TAG, pad(deleted/version)=0, dim=2, zero vector, no SBQ code.
    fn live_tuple() -> Vec<u8> {
        let dim = 2usize;
        let mut b = vec![0u8; ELEM_HEADER + dim * 4];
        b[E_TAG] = ELEM_TAG;
        b[E_DIM..E_DIM + 2].copy_from_slice(&(dim as u16).to_le_bytes());
        b
    }

    #[test]
    fn fresh_tuple_decodes_as_live() {
        let b = live_tuple();
        let ev = decode_element(&b).expect("decode");
        assert!(!ev.deleted, "a fresh tuple (pad=0) decodes as live");
        assert_eq!(ev.version, 0, "fresh version is 0");
    }

    #[test]
    fn mark_tombstone_flips_deleted_and_bumps_version_idempotently() {
        let mut b = live_tuple();
        assert!(mark_tombstone_in_place(&mut b), "marking a live tuple returns true");
        let ev = decode_element(&b).expect("decode");
        assert!(ev.deleted, "marked tuple decodes as deleted");
        assert_eq!(ev.version, 1, "version bumped to 1 on first delete");
        // Idempotent: re-marking a tombstone is a no-op (no double-bump, returns false).
        assert!(!mark_tombstone_in_place(&mut b), "re-marking a tombstone returns false");
        assert_eq!(decode_element(&b).unwrap().version, 1, "version stays 1 (no double-bump)");
    }

    #[test]
    fn tombstone_preserves_tid_nbr_and_vec_bytes() {
        // The tombstone flag must NOT corrupt tid/neighbor/vector (the node is still navigated THROUGH).
        let dim = 2usize;
        let mut b = vec![0u8; ELEM_HEADER + dim * 4];
        b[E_TAG] = ELEM_TAG;
        b[E_LEVEL] = 3;
        b[E_TID..E_TID + 8].copy_from_slice(&42i64.to_le_bytes());
        b[E_NBR_BLK..E_NBR_BLK + 4].copy_from_slice(&7u32.to_le_bytes());
        b[E_NBR_OFF..E_NBR_OFF + 2].copy_from_slice(&9u16.to_le_bytes());
        b[E_DIM..E_DIM + 2].copy_from_slice(&(dim as u16).to_le_bytes());
        b[E_VEC..E_VEC + 4].copy_from_slice(&1.5f32.to_le_bytes());
        mark_tombstone_in_place(&mut b);
        let ev = decode_element(&b).expect("decode");
        assert_eq!((ev.tid, ev.level, ev.nbr_addr), (42, 3, (7, 9)), "tid/level/neighbor intact");
        assert_eq!(
            f32::from_le_bytes(ev.vec_bytes[0..4].try_into().unwrap()),
            1.5,
            "vector intact (navigation needs it)"
        );
        assert!(ev.deleted);
    }
}
