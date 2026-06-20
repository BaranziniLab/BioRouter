"""Value Representation (VR) definitions for DICOM data elements.

Each VR specifies:
  - explicit length: fixed or -1 for variable (2-byte length prefix)
  - padded: whether the value is padded with trailing spaces/nulls
  - numeric: whether the VR holds numeric data (for convenience)
"""

from __future__ import annotations
from dataclasses import dataclass


@dataclass(frozen=True)
class VRInfo:
    """Metadata for a single Value Representation."""
    code: str
    explicit_length: int  # -1 = variable (read 2-byte unsigned length)
    padded: bool = True
    numeric: bool = False


# ── Standard VR catalog (Part 16, Table 6-1) ────────────────────────────────

VR_TABLE: dict[str, VRInfo] = {}

def _add(code: str, explicit: int, padded: bool = True, numeric: bool = False):
    VR_TABLE[code] = VRInfo(code, explicit, padded, numeric)

# Application context
_add("AE", 16)        # Application Entity
_add("AS",  4, False)  # Age String
_add("AT",  4, False)  # Attribute Tag

# String VRs
_add("CS", 16)        # Code String
_add("DA",  8, False)  # Date
_add("DS", 16)        # Decimal String
_add("DT", 26)        # Date Time
_add("IS", 12)        # Integer String
_add("LO", 64)        # Long String
_add("LT", 10240)     # Long Text

_add("FL",  4, False, True)  # Floating Point Single
_add("FD",  8, False, True)  # Floating Point Double

# Binary VRs
_add("OB", -1, False)  # Other Byte String
_add("OD", -1, False)  # Other Double String
_add("OF", -1, False)  # Other Float String
_add("OL", -1, False)  # Other Long String
_add("OW", -1, False)  # Other Word String
_add("OV", -1, False)  # Other 64-bit Very Long String

# Person name
_add("PN", 64)

# Short / structured
_add("SH", 16)        # Short String
_add("SL",  4, False, True)  # Signed Long
_add("SQ", -1, False)  # Sequence of Items (undefined length)
_add("SS",  2, False, True)  # Signed Short
_add("ST", 1024)      # Short Text
_add("SV",  8, False, True)  # Signed 64-bit Very Long
_add("TM", 16)        # Time
_add("UC", -1)        # Unlimited Characters
_add("UI", 64, False)  # Unique Identifier (OID)
_add("UL",  4, False, True)  # Unsigned Long
_add("UN", -1, False)  # Unknown
_add("UR", -1, False)  # URI/URL
_add("US",  2, False, True)  # Unsigned Short
_add("UT", -1)        # Unlimited Text

# 10-byte VRs (extended character repertoire)
_add("UC", -1)  # Unlimited Characters (already added above)
_add("UR", -1)  # URI/URL (already added above)

# ── Lookup helpers ────────────────────────────────────────────────────────────

def get_vr(code: str) -> VRInfo:
    """Return VRInfo for *code*, or a safe unknown default."""
    return VR_TABLE.get(code, VRInfo(code, -1, padded=False))


def vr_name(code: str) -> str:
    """Human-readable name for a VR code."""
    names = {
        "AE": "Application Entity", "AS": "Age String", "AT": "Attribute Tag",
        "CS": "Code String", "DA": "Date", "DS": "Decimal String",
        "DT": "Date Time", "IS": "Integer String", "LO": "Long String",
        "LT": "Long Text", "OB": "Other Byte", "OD": "Other Double",
        "OF": "Other Float", "OL": "Other Long", "OW": "Other Word",
        "OV": "Other 64-bit", "PN": "Person Name", "SH": "Short String",
        "SL": "Signed Long", "SQ": "Sequence", "SS": "Signed Short",
        "ST": "Short Text", "SV": "Signed 64-bit", "TM": "Time",
        "UC": "Unlimited Characters", "UI": "Unique Identifier",
        "UL": "Unsigned Long", "UN": "Unknown", "UR": "URI",
        "US": "Unsigned Short", "UT": "Unlimited Text",
    }
    return names.get(code, f"Unknown ({code})")
