"""M20 parity gate — TheoDB's own Rust distance functions vs pgvector, against the container.

Asserts `theodb.l2_distance` / `theodb.inner_product` / `theodb.cosine_distance` match pgvector's native
`l2_distance` / `inner_product` / `cosine_distance` WITHIN A TIGHT NUMERIC TOLERANCE (ADR D3 — see REL_TOL) on
pgvector's regression oracle rows plus boundary rows from the edge-case review (dim=1, high-dim 1536 +
near-max, NaN/inf, dim-mismatch, NULL). Bit-identical text is not asserted: the installed pgvector uses SIMD
that reorders the f32 sum; TheoDB's scalar f32 differs only in the lowest bits (~1e-6 rel).

Parity is compared against the LIVE installed pgvector (ADR D3 + CK-1): we never hardcode expected constants
— we run BOTH functions on the same input and assert within-tolerance equality, so any pgvector version drift
is caught and the comparison is against the same build. Requires the `theo-db` image
(theodb_rs + pgvector). Connection via PG* env (PGHOST/PGPORT/PGUSER/PGPASSWORD/PGDATABASE).
"""
import math
import os

import psycopg2
import pytest

pytestmark = pytest.mark.integration

# Numeric-parity tolerance (ADR D3): TheoDB's ops accumulate in scalar f32 (the canonical pgvector formula);
# the INSTALLED pgvector accumulates with VECTOR_TARGET_CLONES SIMD, which REORDERS the f32 summation and so
# differs in the lowest bits (pgvector itself varies this across -march). Observed divergence is ~1e-6
# relative at dim 1536. Parity is therefore asserted within a tight relative tolerance, NOT bit-identical text
# (bit-exactness vs a SIMD build is physically impossible without forcing pgvector's scalar path). Integer/
# small cases still match exactly (the tolerance only absorbs SIMD low-bit noise).
REL_TOL = 1e-5


def _parity_ok(theo_val: str, pg_val: str) -> bool:
    """True iff theodb's distance matches pgvector within REL_TOL (handles inf/nan text equality)."""
    if theo_val == pg_val:
        return True
    try:
        t, p = float(theo_val), float(pg_val)
    except (TypeError, ValueError):
        return False
    if math.isinf(t) or math.isinf(p) or math.isnan(t) or math.isnan(p):
        return (math.isinf(t) and math.isinf(p) and (t > 0) == (p > 0)) or (math.isnan(t) and math.isnan(p))
    return abs(t - p) <= REL_TOL * max(1.0, abs(p))

# (label, pgvector_fn, theodb_fn) — theodb.inner_product mirrors pgvector's POSITIVE inner_product; the <#>
# distance is its negation (asserted separately below).
OPS = [
    ("l2", "l2_distance", "theodb.l2_distance"),
    ("ip", "inner_product", "theodb.inner_product"),
    ("cosine", "cosine_distance", "theodb.cosine_distance"),
]

# Oracle + boundary input pairs (text vector literals). High-dim rows are built programmatically below.
PAIRS = [
    ("[0,0]", "[3,4]"),
    ("[0,0]", "[0,1]"),
    ("[1,2]", "[3,4]"),
    ("[1,1,1,1,1,1,1,1,1]", "[1,1,1,1,1,1,1,4,5]"),
    ("[1,0]", "[1,0]"),       # cosine identical → 0
    ("[1,0]", "[0,1]"),       # cosine orthogonal → 1
    ("[1,0]", "[-1,0]"),      # cosine opposite → 2
    ("[3]", "[0]"),           # EC-2: min valid dim
    ("[0.5,0.25,0.125]", "[0.1,0.2,0.3]"),  # non-integer fractions (f32 rounding)
    ("[3e38]", "[3e38]"),     # EC-4: overflow → inf in f32
]


def _conn():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
    )


def _highdim_pair(dim, seed=1):
    # Deterministic pseudo-random f32-ish values (no RNG import needed; stable across runs).
    a = ",".join(str(((i * 7 + seed) % 97) / 13.0) for i in range(dim))
    b = ",".join(str(((i * 5 + seed + 3) % 89) / 11.0) for i in range(dim))
    return (f"[{a}]", f"[{b}]")


@pytest.fixture(scope="module")
def conn():
    c = _conn()
    c.autocommit = True
    yield c
    c.close()


@pytest.mark.parametrize("label,pg_fn,theo_fn", OPS)
def test_distance_matches_pgvector_on_oracle(conn, label, pg_fn, theo_fn):
    """theodb.<op> matches pgvector.<op> within REL_TOL on every oracle + boundary pair (ADR D3)."""
    with conn.cursor() as cur:
        for a, b in PAIRS:
            cur.execute(
                f"SELECT {pg_fn}(%s::vector, %s::vector)::text, {theo_fn}(%s::vector, %s::vector)::text",
                (a, b, a, b),
            )
            pg_val, theo_val = cur.fetchone()
            assert _parity_ok(theo_val, pg_val), f"{label} parity mismatch on ({a},{b}): pgvector={pg_val} theodb={theo_val}"


@pytest.mark.parametrize("dim", [1536, 16000])  # EC-3: 1536 (typical) + near-max VECTOR_MAX_DIM
@pytest.mark.parametrize("label,pg_fn,theo_fn", OPS)
def test_distance_matches_pgvector_highdim(conn, dim, label, pg_fn, theo_fn):
    a, b = _highdim_pair(dim)
    with conn.cursor() as cur:
        cur.execute(
            f"SELECT {pg_fn}(%s::vector, %s::vector)::text, {theo_fn}(%s::vector, %s::vector)::text",
            (a, b, a, b),
        )
        pg_val, theo_val = cur.fetchone()
        assert _parity_ok(theo_val, pg_val), f"{label} highdim({dim}) mismatch: pgvector={pg_val} theodb={theo_val}"


def test_negative_inner_product_is_pgvector_neg_ip(conn):
    """The <#> distance: pgvector's `a <#> b` == -theodb.inner_product(a,b) (the operator is negative IP)."""
    with conn.cursor() as cur:
        for a, b in PAIRS:
            cur.execute(
                "SELECT (%s::vector <#> %s::vector)::text, (-theodb.inner_product(%s::vector, %s::vector))::text",
                (a, b, a, b),
            )
            op_val, theo_neg = cur.fetchone()
            assert _parity_ok(theo_neg, op_val), f"<#> parity mismatch on ({a},{b}): op={op_val} theodb_neg={theo_neg}"


@pytest.mark.parametrize("theo_fn", ["theodb.l2_distance", "theodb.inner_product", "theodb.cosine_distance"])
def test_dim_mismatch_raises(conn, theo_fn):
    """EC-1 negative: mismatched dims fail-fast (theodb raises, like pgvector's CheckDims)."""
    with conn.cursor() as cur:
        with pytest.raises(psycopg2.Error):
            cur.execute(f"SELECT {theo_fn}('[1,2]'::vector, '[3]'::vector)")


@pytest.mark.parametrize("theo_fn", ["theodb.l2_distance", "theodb.inner_product", "theodb.cosine_distance"])
def test_null_arg_returns_null(conn, theo_fn):
    """STRICT parity: NULL vector in → NULL out (matches pgvector's strict distance functions)."""
    with conn.cursor() as cur:
        cur.execute(f"SELECT {theo_fn}(NULL::vector, '[1,2]'::vector)")
        assert cur.fetchone()[0] is None


def test_revoked_from_public(conn):
    with conn.cursor() as cur:
        for fn in ("theodb.l2_distance(vector,vector)", "theodb.inner_product(vector,vector)",
                   "theodb.cosine_distance(vector,vector)"):
            cur.execute("SELECT has_function_privilege('public', %s, 'execute')", (fn,))
            assert cur.fetchone()[0] is False, f"{fn} must be REVOKEd from PUBLIC"
