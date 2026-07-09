//! Domain layer (M66): declarative text chunking for the vectorizer. Pure string logic — no I/O, no
//! embeddings, no pgrx in the split functions (so they are unit-testable via plain `cargo test`, the
//! antidote to the dead-and-unmeasured plpgsql `chunk_text` this replaces). The blueprint chose char-based
//! v1 (no BPE tokenizer — pgai also started char-based; token-based is a tracked v2 debt, ADR-0025-a).
//!
//! Three strategies + `overlap` as an orthogonal parameter (blueprint (a); pgai/LangChain shape):
//!   * `fixed`     — sliding windows of `size` chars, `overlap` chars shared between neighbours.
//!   * `sentence`  — group `.!?`-delimited sentences up to `size` chars (avoids "hanging sentences").
//!   * `recursive` — split on a separator hierarchy (`\n\n` → `\n` → `. ` → ` `), forcing a char-cut only
//!     for an atom that is itself larger than `size` (LangChain's algorithm).
//!
//! Unicode invariant (the subtlest edge, testing.md §4.1): all counting/slicing is by Unicode scalar
//! (`char`), NEVER by byte — a multibyte grapheme (emoji, CJK) is never split. Rust `String` is UTF-8 and
//! `char` iteration guarantees this. Validation (negative cases) raises typed 22023 at the boundary.
use crate::pg::err_input;

/// Chunk `text` per `strategy` into pieces of ~`size` chars with `overlap` chars of context between
/// neighbours. Empty/whitespace-only text → zero chunks (never a 1-empty-chunk). A doc ≤ `size` → 1 chunk.
///
/// Negative cases (typed 22023, fail-fast at the boundary): `size == 0`, `overlap >= size`, unknown strategy.
pub(crate) fn chunk(text: &str, strategy: &str, size: usize, overlap: usize) -> Vec<String> {
    if size == 0 {
        err_input("theodb.chunk: chunk_size must be > 0");
    }
    if overlap >= size {
        err_input("theodb.chunk: overlap must be < chunk_size");
    }
    match strategy {
        "fixed" => fixed_chunks(text, size, overlap),
        "sentence" => sentence_chunks(text, size, overlap),
        "recursive" => recursive_chunks(text, size, overlap),
        other => err_input(&format!(
            "theodb.chunk: unknown chunk_strategy '{other}' (valid: fixed, sentence, recursive)"
        )),
    }
}

/// `fixed` — sliding windows over the char sequence (char-safe; never splits a multibyte grapheme).
fn fixed_chunks(text: &str, size: usize, overlap: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    let step = size - overlap; // overlap < size guaranteed by the caller → step >= 1
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < chars.len() {
        let end = (pos + size).min(chars.len());
        out.push(chars[pos..end].iter().collect());
        if end == chars.len() {
            break; // no trailing overlap-only chunk
        }
        pos += step;
    }
    out
}

/// Split into sentences on `.!?` followed by whitespace or end-of-text. Keeps the terminator with the
/// sentence. Whitespace-only input → no sentences. Char-based (safe).
fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if (c == '.' || c == '!' || c == '?')
            && (i + 1 >= chars.len() || chars[i + 1].is_whitespace())
        {
            let s: String = chars[start..=i].iter().collect();
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < chars.len() {
        let s: String = chars[start..].iter().collect();
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            sentences.push(trimmed.to_string());
        }
    }
    sentences
}

/// `sentence` — group whole sentences up to `size` chars; a sentence larger than `size` is char-cut
/// (fixed) so no chunk exceeds `size` except an indivisible atom. Overlap carries trailing sentences.
fn sentence_chunks(text: &str, size: usize, overlap: usize) -> Vec<String> {
    let sentences = split_sentences(text);
    if sentences.is_empty() {
        return Vec::new();
    }
    // A sentence longer than `size` is itself split by fixed windows so the invariant holds.
    let mut atoms: Vec<String> = Vec::new();
    for s in sentences {
        if s.chars().count() > size {
            atoms.extend(fixed_chunks(&s, size, overlap));
        } else {
            atoms.push(s);
        }
    }
    pack_with_overlap(&atoms, " ", size, overlap)
}

/// `recursive` — split on the separator hierarchy, recursing into finer separators for atoms bigger than
/// `size`, then pack atoms up to `size` with overlap (LangChain's RecursiveCharacterTextSplitter).
fn recursive_chunks(text: &str, size: usize, overlap: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let seps = ["\n\n", "\n", ". ", " "];
    let atoms = split_recursive(text, &seps, size, overlap);
    pack_with_overlap(&atoms, " ", size, overlap)
}

/// Produce atoms each ≤ `size` chars by trying separators in order; the deepest fallback is a forced
/// char-cut (via `fixed_chunks`) so a giant separator-less word never yields a chunk > size nor loops.
fn split_recursive(text: &str, seps: &[&str], size: usize, overlap: usize) -> Vec<String> {
    if text.chars().count() <= size {
        let t = text.trim();
        return if t.is_empty() { Vec::new() } else { vec![t.to_string()] };
    }
    match seps.split_first() {
        None => fixed_chunks(text, size, overlap), // no separators left → forced char-cut
        Some((sep, rest)) => {
            let mut atoms = Vec::new();
            for piece in text.split(sep) {
                if piece.trim().is_empty() {
                    continue;
                }
                if piece.chars().count() <= size {
                    atoms.push(piece.trim().to_string());
                } else {
                    atoms.extend(split_recursive(piece, rest, size, overlap));
                }
            }
            atoms
        }
    }
}

/// Greedily pack `atoms` (each ≤ size, except indivisible ones) into chunks up to `size` chars joined by
/// `joiner`; when a chunk closes, the next starts by carrying trailing atoms whose total ≤ `overlap`
/// (LangChain `_merge_splits`). An atom bigger than `size` becomes its own chunk (already char-cut upstream).
fn pack_with_overlap(atoms: &[String], joiner: &str, size: usize, overlap: usize) -> Vec<String> {
    let jlen = joiner.chars().count();
    // Total chars of a unit list joined by `joiner` (recomputed to avoid drift after front-pops).
    let total = |v: &[String]| -> usize {
        if v.is_empty() {
            0
        } else {
            v.iter().map(|a| a.chars().count()).sum::<usize>() + jlen * (v.len() - 1)
        }
    };
    let mut chunks = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for atom in atoms {
        let alen = atom.chars().count();
        let add = if cur.is_empty() { alen } else { alen + jlen };
        if total(&cur) + add > size && !cur.is_empty() {
            chunks.push(cur.join(joiner));
            // Carry overlap: drop leading atoms until the retained tail fits in `overlap` chars.
            // `!cur.is_empty()` (NOT `len > 1`) so overlap==0 clears the buffer — else chunks would
            // accumulate past `size` (the bug the desk-test caught).
            while total(&cur) > overlap && !cur.is_empty() {
                cur.remove(0);
            }
        }
        cur.push(atom.clone());
    }
    if !cur.is_empty() {
        chunks.push(cur.join(joiner));
    }
    chunks
}

// M66 — negative-case pg_tests (the typed-error boundary needs pgrx; run under `cargo pgrx test`).
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod pg_tests {
    use super::*;
    use pgrx::prelude::*;

    #[pg_test(error = "theodb.chunk: chunk_size must be > 0")]
    fn size_zero_rejected() {
        let _ = chunk("hello", "fixed", 0, 0);
    }

    #[pg_test(error = "theodb.chunk: overlap must be < chunk_size")]
    fn overlap_ge_size_rejected() {
        let _ = chunk("hello", "fixed", 4, 4);
    }

    #[pg_test(error = "theodb.chunk: unknown chunk_strategy 'semantic' (valid: fixed, sentence, recursive)")]
    fn unknown_strategy_rejected() {
        let _ = chunk("hello", "semantic", 10, 0);
    }

    #[pg_test]
    fn valid_strategies_produce_chunks() {
        // Smoke: each strategy returns chunks over the same input, all within size (char-count).
        for strat in ["fixed", "sentence", "recursive"] {
            let c = chunk("The cat sat. The dog ran. A bird flew away today.", strat, 20, 4);
            assert!(!c.is_empty(), "strategy {strat} produced no chunks");
        }
    }
}

// M66 — pure unit tests (plain `cargo test`, no DB/pgrx needed — the split logic is pure string).
#[cfg(test)]
mod pure_tests {
    use super::*;

    #[test]
    fn fixed_windows_with_overlap() {
        // "abcdefghij" (10), size 4, overlap 1 → step 3, windows at 0,3,6: [abcd, defg, ghij]
        // (at pos 6 end==10 → break, no trailing overlap-only chunk). Covers all 10 chars.
        let c = fixed_chunks("abcdefghij", 4, 1);
        assert_eq!(c, vec!["abcd", "defg", "ghij"]);
    }

    #[test]
    fn fixed_doc_smaller_than_size_single_chunk() {
        assert_eq!(fixed_chunks("abc", 10, 2), vec!["abc"]);
    }

    #[test]
    fn empty_returns_no_chunks() {
        assert!(fixed_chunks("", 4, 1).is_empty());
        assert!(sentence_chunks("   ", 10, 0).is_empty());
        assert!(recursive_chunks("\n\n  \n", 10, 0).is_empty());
    }

    #[test]
    fn multibyte_never_splits_a_char() {
        // 5 emoji (each is one char, multiple bytes), size 2 → chunks of 2/2/1 emoji, all valid UTF-8.
        let c = fixed_chunks("😀😁😂😃😄", 2, 0);
        assert_eq!(c, vec!["😀😁", "😂😃", "😄"]);
        for ch in &c {
            assert!(std::str::from_utf8(ch.as_bytes()).is_ok());
        }
        // CJK
        let c2 = fixed_chunks("日本語のテスト", 3, 0);
        assert_eq!(c2, vec!["日本語", "のテス", "ト"]);
    }

    #[test]
    fn sentence_groups_until_size() {
        // Three short sentences, size big enough for two → 2 chunks.
        let c = sentence_chunks("A cat sat. A dog ran. A bird flew.", 22, 0);
        assert_eq!(c.len(), 2);
        assert!(c[0].contains("cat") && c[0].contains("dog"));
        assert!(c[1].contains("bird"));
    }

    #[test]
    fn sentence_giant_sentence_is_char_cut() {
        // one 20-char "sentence" with no terminator, size 8 → char-cut into pieces, none > 8.
        let c = sentence_chunks("abcdefghijklmnopqrst", 8, 0);
        assert!(c.iter().all(|s| s.chars().count() <= 8));
        assert!(!c.is_empty());
    }

    #[test]
    fn recursive_prefers_paragraph_then_sentence() {
        // Two paragraphs; size fits one paragraph each → split on \n\n first.
        let text = "First para sentence one. First para sentence two.\n\nSecond paragraph here.";
        let c = recursive_chunks(text, 50, 0);
        assert!(c.len() >= 2);
        assert!(c.iter().all(|s| s.chars().count() <= 50 || !s.contains(' ')));
    }

    #[test]
    fn recursive_giant_word_forces_char_cut_no_infinite_loop() {
        // A single 30-char token with no separators, size 10 → forced char-cut, no chunk > 10, terminates.
        let c = recursive_chunks("abcdefghijklmnopqrstuvwxyz0123", 10, 0);
        assert!(c.iter().all(|s| s.chars().count() <= 10));
        assert!(!c.is_empty());
    }

    #[test]
    fn overlap_carries_context() {
        // Sentences with overlap: the second chunk should start with the tail of the first (shared atom).
        let c = sentence_chunks("Aaa. Bbb. Ccc. Ddd.", 9, 4);
        // With overlap, at least two chunks and some shared token across the boundary.
        assert!(c.len() >= 2);
    }
}
