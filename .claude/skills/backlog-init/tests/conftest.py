"""Põe scripts/ da slice no sys.path, como nas demais slices."""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "scripts"))
