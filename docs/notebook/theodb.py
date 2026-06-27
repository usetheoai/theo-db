"""TheoDB — SDK Python (fachada de marca sobre a stack OSS).

O TheoDB é PostgreSQL 17 + ``pgvector``. Esta é a **superfície estável de produto**:
``TheoDBEngine`` / ``TheoDBVectorStore`` / ``TheoDBHNSWIndex`` … são nomes de marca que
re-exportam a integração OSS permissiva ``langchain-postgres`` (``PGEngine`` / ``PGVectorStore``).

Princípio (CLAUDE.md — "Esforço ≠ Complexidade" + Regra 9 "não reinvente"): a marca é
complexidade **essencial** (produto); a fachada é fina (aliases), nunca uma reimplementação.
A implementação por baixo pode evoluir sem quebrar o código de quem usa ``theodb``.

Status honesto (Regra 3): fachada de referência **em-repo**; ainda não publicada no PyPI.
Requer ``langchain-postgres`` instalado (ver a célula de instalação do notebook).
"""

from langchain_postgres import PGEngine as TheoDBEngine
from langchain_postgres import PGVectorStore as TheoDBVectorStore
from langchain_postgres import Column as TheoDBColumn
from langchain_postgres.indexes import HNSWIndex as TheoDBHNSWIndex
from langchain_postgres.indexes import IVFFlatIndex as TheoDBIVFFlatIndex

# Hybrid search (RRF) — alvo de roadmap M7; requer uma versão recente de langchain-postgres.
try:
    from langchain_postgres import HybridSearchConfig as TheoDBHybridSearchConfig
    from langchain_postgres import reciprocal_rank_fusion
except ImportError:  # pragma: no cover - depende da versão instalada
    TheoDBHybridSearchConfig = None
    reciprocal_rank_fusion = None

__all__ = [
    "TheoDBEngine",
    "TheoDBVectorStore",
    "TheoDBColumn",
    "TheoDBHNSWIndex",
    "TheoDBIVFFlatIndex",
    "TheoDBHybridSearchConfig",
    "reciprocal_rank_fusion",
]
