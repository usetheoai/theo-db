"""Coverage-rot guard: no packaging file may pin a PostgreSQL major other than the engine's.

Why this exists: the engine moved to PG18 (`Dockerfile`: `postgres:18-bookworm`, `PG_MAJOR=18`) but
three CI-only images kept PG17 pins, and each one broke a different job while *looking* correct:

  packaging/run-regress.sh      BIN=/usr/lib/postgresql/17/bin  -> path does not exist on a PG18
                                image, so `$BIN/postgres --version` returned the EMPTY string and the
                                guard reported the baffling `engine is not 18.4 (got: )`
  packaging/Dockerfile.regress  ARG PG_TAG=REL_17_10            -> built pg_regress from PG17 source
                                against a PG18 server (expected outputs differ across majors)
  packaging/Dockerfile.bm25     postgresql-server-dev-17        -> PG17 headers, PG18 pg_config; the
  packaging/Dockerfile.m53-bm25                                    extension `make` died with exit 2

Every one of those files ALSO carried prose claiming PG18 ("runs the upstream PostgreSQL 18.4
regression suite"), so a reader — and a reviewer — saw agreement where the build saw contradiction.
That gap is exactly what a version bump leaves behind, and nothing in the pipeline noticed for as
long as those jobs were red for other reasons too.

HONESTLY HEURISTIC: this greps for PG-major-shaped literals in packaging/. It cannot catch a major
encoded indirectly (computed at build time, read from a file). It is a ratchet against the literal
pin — the form every one of the four defects above actually took.

Comments are stripped before scanning: a `#` line describing the OLD pin ("the path
/usr/lib/postgresql/17/bin stopped existing") is documentation, not a pin, and flagging it would
punish exactly the comment that explains the fix. The stripping is naive (`#` to end of line) and
would mis-handle a `#` inside a quoted string; no packaging file does that today.
"""
import pathlib
import re

REPO = pathlib.Path(__file__).resolve().parents[2]
DOCKERFILE = REPO / "Dockerfile"
PACKAGING = REPO / "packaging"

# Literals that pin a PG major. Each entry is (regex, human description).
PINS = [
    (re.compile(r"postgresql-server-dev-(\d+)"), "postgresql-server-dev-N apt package"),
    (re.compile(r"/usr/lib/postgresql/(\d+)/"), "/usr/lib/postgresql/N/ path"),
    (re.compile(r"REL_(\d+)_\d+"), "REL_N_M upstream source tag"),
]


def strip_comments(source):
    """Drop `#` comments so the guard asserts on directives, not on prose about them."""
    return "\n".join(line.split("#", 1)[0] for line in source.splitlines())


def engine_pg_major():
    """The single source of truth: PG_MAJOR in the shipped Dockerfile."""
    match = re.search(r"^ARG PG_MAJOR=(\d+)", DOCKERFILE.read_text(), re.M)
    assert match, "Dockerfile no longer declares `ARG PG_MAJOR=<n>` — this guard needs updating"
    return match.group(1)


def test_engine_pg_major_is_discoverable():
    """Given the shipped Dockerfile, the engine major is a plain integer.

    Guards the guard: if this ever fails, every assertion below is vacuous.
    """
    assert engine_pg_major().isdigit()


def test_no_packaging_file_pins_a_foreign_pg_major():
    """Given any file under packaging/, every PG-major literal matches the engine's.

    A mismatch does not fail the image build loudly — it produces an empty version string, a
    missing header, or a silently wrong expected-output baseline.
    """
    expected = engine_pg_major()
    offenders = []

    for path in sorted(PACKAGING.rglob("*")):
        if not path.is_file():
            continue
        try:
            source = path.read_text()
        except UnicodeDecodeError:
            continue  # binary asset — no version pin to read
        for pattern, description in PINS:
            for found in pattern.finditer(strip_comments(source)):
                if found.group(1) != expected:
                    offenders.append(
                        f"{path.relative_to(REPO)}: {description} pins PG{found.group(1)} "
                        f"(engine is PG{expected}) -> {found.group(0)!r}"
                    )

    assert offenders == [], (
        f"packaging/ pins a PostgreSQL major that is not the engine's (PG{expected}):\n  "
        + "\n  ".join(offenders)
    )
