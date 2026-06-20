"""Pure-Python DICOM Part-10 file reader.

Parses preamble → DICM magic → File Meta Information (explicit VR, LE) →
Data Set (explicit or implicit VR depending on Transfer Syntax) including
nested sequences.

Supports:
  - Explicit VR Little Endian (1.2.840.10008.1.2.1)
  - Implicit VR Little Endian (1.2.840.10008.1.2)
  - Explicit VR Big Endian (1.2.840.10008.1.2.2)
"""

from __future__ import annotations

import struct
from dataclasses import dataclass, field
from io import BytesIO
from pathlib import Path
from typing import Any, BinaryIO, Dict, Iterator, List, Optional, Tuple, Union

from medicom.dicom.vr import get_vr, VRInfo, vr_name
from medicom.dicom.tags import (
    Tag, TAGS, TagInfo,
    TRANSFER_SYNTAX_UID,
    FILE_META_INFO_VERSION,
    PIXEL_DATA,
    ITEM, ITEM_DELIMITATION, SEQUENCE_DELIMITATION,
    tag_by_keyword,
)


# ── Constants ────────────────────────────────────────────────────────────────

DICM_MAGIC = b"DICM"
PREAMBLE_LENGTH = 128

# Transfer Syntax UIDs
TS_IMPLICIT_LE = "1.2.840.10008.1.2"
TS_EXPLICIT_LE = "1.2.840.10008.1.2.1"
TS_EXPLICIT_BE = "1.2.840.10008.1.2.2"

# Deflated transfer syntax
TS_DEFLATEDExplicit_LE = "1.2.840.10008.1.2.1.99"
TS_JPEG2000_LOSSLESS    = "1.2.840.10008.1.2.4.90"
TS_JPEG2000_LOSSY       = "1.2.840.10008.1.2.4.91"
TS_JPEG_LOSSY           = "1.2.840.10008.1.2.4.50"
TS_JPEG_LOSSLESS        = "1.2.840.10008.1.2.4.57"


# ── Data classes ─────────────────────────────────────────────────────────────

@dataclass
class DataElement:
    """A single DICOM data element."""
    tag: Tag
    vr: str
    length: int
    value: Any = None          # decoded Python object
    raw_bytes: bytes = b""    # raw value bytes
    is_undefined_length: bool = False
    sequence_items: Optional[List[Any]] = None  # for SQ elements

    @property
    def keyword(self) -> str:
        if self.tag in TAGS:
            return TAGS[self.tag].keyword
        return self.tag.hex

    @property
    def name(self) -> Optional[str]:
        if self.tag in TAGS:
            return TAGS[self.tag].name
        return None

    def value_as_str(self) -> str:
        """Attempt to decode the value as a string."""
        if self.value is not None:
            if isinstance(self.value, str):
                return self.value
            if isinstance(self.value, list):
                return "\\".join(str(v) for v in self.value)
            return str(self.value)
        return self.raw_bytes.decode("ascii", errors="replace").strip("\x00 ")


@dataclass
class DICOMDataset:
    """Container for parsed DICOM data elements, indexed by Tag."""
    elements: Dict[Tag, DataElement] = field(default_factory=dict)
    file_meta: Dict[Tag, DataElement] = field(default_factory=dict)
    transfer_syntax: str = TS_EXPLICIT_LE
    is_explicit_vr: bool = True
    is_little_endian: bool = True

    def __getitem__(self, tag: Tag) -> DataElement:
        return self.elements[tag]

    def get(self, tag: Tag, default: Any = None) -> Optional[DataElement]:
        return self.elements.get(tag, default)

    def get_value(self, tag: Tag, default: Any = None) -> Any:
        elem = self.elements.get(tag)
        if elem is None:
            return default
        return elem.value

    def get_str(self, tag: Tag, default: str = "") -> str:
        elem = self.elements.get(tag)
        if elem is None:
            return default
        v = elem.value_as_str()
        return v if v else default

    def get_int(self, tag: Tag, default: int = 0) -> int:
        v = self.get_value(tag)
        if v is None:
            return default
        try:
            if isinstance(v, list):
                return int(v[0]) if v else default
            return int(v)
        except (TypeError, ValueError):
            return default

    def get_float(self, tag: Tag, default: float = 0.0) -> float:
        v = self.get_value(tag)
        if v is None:
            return default
        try:
            if isinstance(v, list):
                return float(v[0]) if v else default
            return float(v)
        except (TypeError, ValueError):
            return default

    def has(self, tag: Tag) -> bool:
        return tag in self.elements

    def __contains__(self, tag: Tag) -> bool:
        return tag in self.elements

    def __iter__(self):
        return iter(self.elements.values())

    def tags(self) -> Iterator[Tag]:
        return iter(self.elements.keys())

    def items(self) -> Iterator[Tuple[Tag, DataElement]]:
        return self.elements.items()


class DICOMFile:
    """High-level DICOM file reader.

    Usage::

        dcm = DICOMFile.from_path("scan.dcm")
        patient = dcm.dataset.get_str(PATIENT_NAME)
        pixels = dcm.pixel_array()
    """

    def __init__(self):
        self.path: Optional[Path] = None
        self.file_meta = DICOMDataset()
        self.dataset = DICOMDataset()
        self._pixel_bytes: Optional[bytes] = None

    @classmethod
    def from_path(cls, path: Union[str, Path]) -> "DICOMFile":
        path = Path(path)
        with open(path, "rb") as f:
            return cls._parse(f, path=path)

    @classmethod
    def from_bytes(cls, data: bytes) -> "DICOMFile":
        return cls._parse(BytesIO(data))

    @classmethod
    def _parse(cls, stream: BinaryIO, path: Optional[Path] = None) -> "DICOMFile":
        dcm = cls()
        dcm.path = path

        # ── 1. Preamble (128 bytes, ignored) ──────────────────────────────
        preamble = stream.read(PREAMBLE_LENGTH)
        if len(preamble) < PREAMBLE_LENGTH:
            raise ValueError("File too short for DICOM preamble")

        # ── 2. DICM magic ─────────────────────────────────────────────────
        magic = stream.read(4)
        if magic != DICM_MAGIC:
            raise ValueError(
                f"Missing DICM magic bytes — got {magic!r} at offset 128"
            )

        # ── 3. File Meta Information (always explicit VR, little endian) ──
        # Read the meta info: first element is always (0002,0000) Group Length
        # which tells us how many bytes follow. We read the group length,
        # then parse exactly that many bytes as meta elements.
        meta_start_pos = stream.tell()
        meta_elements = _read_meta_group_length(stream, little_endian=True)
        dcm.file_meta.elements = {e.tag: e for e in meta_elements}

        # Determine transfer syntax
        ts_elem = dcm.file_meta.get(TRANSFER_SYNTAX_UID)
        ts_uid = ts_elem.value_as_str().strip("\x00 ") if ts_elem else TS_EXPLICIT_LE
        dcm.dataset.transfer_syntax = ts_uid
        dcm.file_meta.transfer_syntax = ts_uid

        # Determine VR and endianness for dataset
        if ts_uid in (TS_IMPLICIT_LE,):
            explicit_vr = False
            little_endian = True
        elif ts_uid in (TS_EXPLICIT_LE, TS_DEFLATEDExplicit_LE):
            explicit_vr = True
            little_endian = True
        elif ts_uid == TS_EXPLICIT_BE:
            explicit_vr = True
            little_endian = False
        else:
            # Default to explicit VR LE for compressed — we'll read what we can
            explicit_vr = True
            little_endian = True

        dcm.dataset.is_explicit_vr = explicit_vr
        dcm.dataset.is_little_endian = little_endian

        # Handle deflated transfer syntax
        if ts_uid == TS_DEFLATEDExplicit_LE:
            import zlib
            # Skip 2 bytes (deflate encapsulation header)
            stream.read(2)
            raw = stream.read()
            try:
                decompressed = zlib.decompress(raw, -15)  # raw deflate
                stream = BytesIO(decompressed)
            except Exception:
                stream = BytesIO(raw)

        # ── 4. Dataset ────────────────────────────────────────────────────
        elements = _read_data_elements(
            stream,
            explicit_vr=explicit_vr,
            little_endian=little_endian,
            max_tag_group=None,
        )
        dcm.dataset.elements = {e.tag: e for e in elements}

        # Store pixel data raw bytes if present
        pixel_elem = dcm.dataset.get(PIXEL_DATA)
        if pixel_elem:
            dcm._pixel_bytes = pixel_elem.raw_bytes

        return dcm

    def pixel_array(self):
        """Return pixel data as a flat bytes object (no decompression)."""
        if self._pixel_bytes is None:
            raise ValueError("No pixel data in this DICOM file")
        return self._pixel_bytes

    def has_pixel_data(self) -> bool:
        return self._pixel_bytes is not None and len(self._pixel_bytes) > 0

    def summary(self) -> str:
        """Return a human-readable header summary."""
        lines = ["DICOM Header Summary", "=" * 40]
        fields = [
            ("Patient Name",      Tag(0x0010, 0x0010)),
            ("Patient ID",        Tag(0x0010, 0x0020)),
            ("Patient Sex",       Tag(0x0010, 0x0040)),
            ("Patient Birth",     Tag(0x0010, 0x0030)),
            ("Study Date",        Tag(0x0008, 0x0020)),
            ("Study Instance UID",Tag(0x0020, 0x000D)),
            ("Series Instance UID",Tag(0x0020, 0x000E)),
            ("Modality",          Tag(0x0008, 0x0060)),
            ("Instance Number",   Tag(0x0020, 0x0013)),
            ("Rows",              Tag(0x0028, 0x0010)),
            ("Columns",           Tag(0x0028, 0x0011)),
            ("Bits Allocated",    Tag(0x0028, 0x0100)),
            ("Bits Stored",       Tag(0x0028, 0x0101)),
            ("Pixel Spacing",     Tag(0x0028, 0x0030)),
            ("Window Center",     Tag(0x0028, 0x1050)),
            ("Window Width",      Tag(0x0028, 0x1051)),
            ("Rescale Slope",     Tag(0x0028, 0x1053)),
            ("Rescale Intercept", Tag(0x0028, 0x1052)),
            ("SOP Class UID",     Tag(0x0008, 0x0016)),
            ("SOP Instance UID",  Tag(0x0008, 0x0018)),
        ]
        for label, tag in fields:
            val = self.dataset.get_str(tag, "—")
            lines.append(f"  {label:.<30s} {val}")
        lines.append(f"  {'Transfer Syntax':.<30s} {self.dataset.transfer_syntax}")
        lines.append(f"  {'Has Pixel Data':.<30s} {'Yes' if self.has_pixel_data() else 'No'}")
        if self.has_pixel_data():
            lines.append(f"  {'Pixel Data Size':.<30s} {len(self._pixel_bytes)} bytes")
        return "\n".join(lines)


# ── Low-level element readers ────────────────────────────────────────────────

def _read_meta_group_length(stream: BinaryIO, little_endian: bool) -> List[DataElement]:
    """Read File Meta Information starting with Group Length element.

    The first element is always (0002,0000) Group Length with VR=UL.
    Its value tells us how many bytes of meta elements follow.
    We read the group length, then parse exactly that many bytes as meta elements.
    """
    fmt = "<" if little_endian else ">"

    # Read tag (0002,0000)
    tag = _read_tag(stream, little_endian)
    if tag.group != 0x0002 or tag.element != 0x0000:
        raise ValueError(f"Expected FileMetaInformationGroupLength (0002,0000), got {tag.hex}")

    # Read VR "UL" (explicit VR, always)
    vr_raw = stream.read(2)
    if len(vr_raw) < 2:
        raise ValueError("Truncated File Meta Information")
    vr = vr_raw.decode("ascii")

    # Read 2-byte length for UL
    raw_len = stream.read(2)
    if len(raw_len) < 2:
        raise ValueError("Truncated File Meta Information")
    group_length = struct.unpack(f"{fmt}H", raw_len)[0]

    # Read exactly group_length bytes as meta elements
    meta_data = stream.read(group_length)
    if len(meta_data) < group_length:
        raise ValueError(f"Truncated File Meta Information: expected {group_length} bytes")

    # Create the group length data element
    gl_elem = DataElement(
        tag=tag, vr="UL", length=group_length,
        value=group_length, raw_bytes=struct.pack(f"{fmt}I", group_length),
    )

    # Parse the meta elements from the bytes
    meta_stream = BytesIO(meta_data)
    meta_elements = _read_data_elements(
        meta_stream,
        explicit_vr=True,
        little_endian=little_endian,
        max_tag_group=0x0002,
    )

    return [gl_elem] + meta_elements


def _read_tag(stream: BinaryIO, little_endian: bool) -> Tag:
    raw = stream.read(4)
    if len(raw) < 4:
        raise ValueError("Unexpected end of file while reading tag")
    fmt = "<HH" if little_endian else ">HH"
    g, e = struct.unpack(fmt, raw)
    return Tag(g, e)


def _read_ui_value(raw: bytes) -> str:
    """Clean a UI value: strip trailing nulls/spaces."""
    return raw.decode("ascii", errors="replace").strip("\x00 ")


def _decode_string_value(raw: bytes) -> str:
    """Decode a string VR value."""
    try:
        s = raw.decode("ascii")
    except UnicodeDecodeError:
        s = raw.decode("latin-1")
    # Strip padding
    s = s.rstrip("\x00 ")
    return s


def _decode_value(raw: bytes, vr: str) -> Any:
    """Decode raw bytes into a Python value based on VR."""
    if not raw:
        return ""

    vr_info = get_vr(vr)

    if vr == "UI":
        return _read_ui_value(raw)
    elif vr in ("LO", "SH", "CS", "IS", "DS", "DA", "TM", "AE", "AS", "LT", "ST", "UT", "UC"):
        return _decode_string_value(raw)
    elif vr == "PN":
        # Person Name: components separated by ^, groups separated by =
        s = _decode_string_value(raw)
        return s
    elif vr == "US" and len(raw) >= 2:
        return list(struct.unpack(f"<{len(raw)//2}H" if True else f">{len(raw)//2}H", raw))
    elif vr == "SS" and len(raw) >= 2:
        return list(struct.unpack(f"<{len(raw)//2}h" if True else f">{len(raw)//2}h", raw))
    elif vr == "UL" and len(raw) >= 4:
        return list(struct.unpack(f"<{len(raw)//4}I" if True else f">{len(raw)//4}I", raw))
    elif vr == "SL" and len(raw) >= 4:
        return list(struct.unpack(f"<{len(raw)//4}i" if True else f">{len(raw)//4}i", raw))
    elif vr == "FL" and len(raw) >= 4:
        return list(struct.unpack(f"<{len(raw)//4}f" if True else f">{len(raw)//4}f", raw))
    elif vr == "FD" and len(raw) >= 8:
        return list(struct.unpack(f"<{len(raw)//8}d" if True else f">{len(raw)//8}d", raw))
    elif vr in ("OB", "OW", "OF", "OD", "OL", "OV", "UN", "AT"):
        return raw
    elif vr == "SQ":
        return raw  # sequences handled separately
    else:
        return raw


def _read_data_elements(
    stream: BinaryIO,
    explicit_vr: bool,
    little_endian: bool,
    max_tag_group: Optional[int] = None,
    until_tag: Optional[Tag] = None,
    until_byte: Optional[int] = None,
) -> List[DataElement]:
    """Read data elements from a stream.

    Parameters
    ----------
    max_tag_group : if set, stop when group exceeds this (for meta info).
    until_tag : if set, stop before reading this tag.
    until_byte : if set, stop when stream position reaches this byte.
    """
    elements: List[DataElement] = []
    fmt = "<" if little_endian else ">"

    while True:
        # Check bounds
        if until_byte is not None:
            pos = stream.tell()
            if pos >= until_byte:
                break

        # Check for stream exhaustion (at least 4 bytes needed for a tag)
        pos_before = stream.tell()
        peek = stream.read(4)
        if len(peek) < 4:
            break
        stream.seek(pos_before)

        # Read tag
        tag = _read_tag(stream, little_endian)

        # Stop conditions
        if max_tag_group is not None and tag.group > max_tag_group:
            # Seek back — we overshot
            stream.seek(-4, 1)
            break
        if until_tag is not None and tag == until_tag:
            break

        # Item / sequence delimiters
        if tag == ITEM or tag == ITEM_DELIMITATION or tag == SEQUENCE_DELIMITATION:
            # These are handled by the sequence reader — return what we have
            # and let the caller decide
            stream.seek(-4, 1)
            break

        # Read VR
        if explicit_vr:
            vr_raw = stream.read(2)
            if len(vr_raw) < 2:
                break
            vr = vr_raw.decode("ascii", errors="replace")
        else:
            # Implicit VR — look up from tag table
            if tag in TAGS and TAGS[tag].vr:
                vr = TAGS[tag].vr
            else:
                vr = "UN"

        vr_info = get_vr(vr)

        # Read value length
        LONG_VR_CODES = ("OB", "OW", "OF", "OD", "OL", "SQ", "UN", "UC", "UR", "OV", "AT")

        if explicit_vr:
            if vr in LONG_VR_CODES:
                # Explicit VR long format: 2 reserved bytes + 4-byte length
                reserved = stream.read(2)
                raw_len = stream.read(4)
                if len(raw_len) < 4:
                    break
                length = struct.unpack(f"{fmt}I", raw_len)[0]
            else:
                # Explicit VR short format: 2-byte length
                raw_len = stream.read(2)
                if len(raw_len) < 2:
                    break
                length = struct.unpack(f"{fmt}H", raw_len)[0]
        else:
            # Implicit VR: always 4-byte length
            raw_len = stream.read(4)
            if len(raw_len) < 4:
                break
            length = struct.unpack(f"{fmt}I", raw_len)[0]

        is_undefined = (length == 0xFFFFFFFF)

        # Read value
        if is_undefined:
            # Sequence with undefined length — read until sequence delimiter
            if vr == "SQ" or (tag in TAGS and TAGS[tag].vr == "SQ"):
                items = _read_sequence_items(stream, little_endian, explicit_vr)
                elem = DataElement(
                    tag=tag, vr="SQ", length=length, value=items,
                    raw_bytes=b"", is_undefined_length=True,
                    sequence_items=items,
                )
            else:
                # Read until item delimiter
                raw_value, items = _read_undefined_length_data(stream, little_endian, explicit_vr)
                elem = DataElement(
                    tag=tag, vr=vr, length=length, value=raw_value,
                    raw_bytes=raw_value, is_undefined_length=True,
                    sequence_items=items,
                )
        else:
            raw_value = stream.read(length)
            if len(raw_value) < length:
                # Pad with zeros
                raw_value = raw_value + b"\x00" * (length - len(raw_value))

            if vr == "SQ" or (tag in TAGS and TAGS[tag].vr == "SQ"):
                # Sequence with defined length
                items = _read_sequence_items_from_bytes(raw_value, little_endian, explicit_vr)
                elem = DataElement(
                    tag=tag, vr="SQ", length=length, value=items,
                    raw_bytes=raw_value, is_undefined_length=False,
                    sequence_items=items,
                )
            elif tag == PIXEL_DATA or (tag.group == 0x7FE0 and tag.element == 0x0010):
                elem = DataElement(
                    tag=tag, vr=vr, length=length, value=None,
                    raw_bytes=raw_value,
                )
            else:
                decoded = _decode_value(raw_value, vr)
                elem = DataElement(
                    tag=tag, vr=vr, length=length, value=decoded,
                    raw_bytes=raw_value,
                )

        elements.append(elem)

    return elements


def _read_sequence_items(
    stream: BinaryIO,
    little_endian: bool,
    explicit_vr: bool,
) -> List[List[DataElement]]:
    """Read sequence items for undefined-length SQ."""
    items: List[List[DataElement]] = []
    fmt = "<" if little_endian else ">"

    while True:
        tag = _read_tag(stream, little_endian)
        raw_len = stream.read(4)
        if len(raw_len) < 4:
            break
        length = struct.unpack(f"{fmt}I", raw_len)[0]

        if tag == SEQUENCE_DELIMITATION:
            break
        if tag == ITEM_DELIMITATION:
            continue
        if tag != ITEM:
            # Unexpected tag — seek back
            stream.seek(-8, 1)
            break

        if length == 0xFFFFFFFF:
            # Undefined-length item
            item_elements = _read_data_elements(
                stream,
                explicit_vr=explicit_vr,
                little_endian=little_endian,
                until_tag=ITEM_DELIMITATION,
            )
            # Consume delimiter
            d_tag = _read_tag(stream, little_endian)
            if d_tag != ITEM_DELIMITATION:
                stream.seek(-4, 1)
            # Read delimiter length (should be 0)
            stream.read(4)
            items.append(item_elements)
        else:
            if length == 0:
                items.append([])
                continue
            # Defined-length item
            item_data = stream.read(length)
            item_stream = BytesIO(item_data)
            item_elements = _read_data_elements(
                item_stream,
                explicit_vr=explicit_vr,
                little_endian=little_endian,
            )
            items.append(item_elements)

    return items


def _read_undefined_length_data(
    stream: BinaryIO,
    little_endian: bool,
    explicit_vr: bool,
) -> Tuple[bytes, List[List[DataElement]]]:
    """Read undefined-length non-SQ data (e.g., pixel data encapsulation)."""
    fmt = "<" if little_endian else ">"
    raw_chunks: List[bytes] = []
    items: List[List[DataElement]] = []

    while True:
        tag = _read_tag(stream, little_endian)
        raw_len = stream.read(4)
        if len(raw_len) < 4:
            break
        length = struct.unpack(f"{fmt}I", raw_len)[0]

        if tag == SEQUENCE_DELIMITATION:
            break
        if tag == ITEM_DELIMITATION:
            continue
        if tag == ITEM:
            if length == 0xFFFFFFFF:
                # Encapsulated fragment sequence
                item_elements = _read_data_elements(
                    stream, explicit_vr, little_endian,
                    until_tag=ITEM_DELIMITATION,
                )
                for ie in item_elements:
                    if ie.raw_bytes:
                        raw_chunks.append(ie.raw_bytes)
                # Consume delimiter
                d_tag = _read_tag(stream, little_endian)
                stream.read(4)  # delimiter length
                items.append(item_elements)
            else:
                chunk = stream.read(length)
                raw_chunks.append(chunk)
        else:
            stream.seek(-8, 1)
            break

    return b"".join(raw_chunks), items


def _read_sequence_items_from_bytes(
    data: bytes,
    little_endian: bool,
    explicit_vr: bool,
) -> List[List[DataElement]]:
    """Parse items from a defined-length SQ's raw bytes."""
    stream = BytesIO(data)
    items: List[List[DataElement]] = []
    fmt = "<" if little_endian else ">"

    while stream.tell() < len(data):
        tag = _read_tag(stream, little_endian)
        raw_len = stream.read(4)
        if len(raw_len) < 4:
            break
        length = struct.unpack(f"{fmt}I", raw_len)[0]

        if tag == SEQUENCE_DELIMITATION:
            break
        if tag == ITEM_DELIMITATION:
            continue
        if tag != ITEM:
            stream.seek(-8, 1)
            break

        if length == 0xFFFFFFFF:
            item_elements = _read_data_elements(
                stream, explicit_vr, little_endian,
                until_tag=ITEM_DELIMITATION,
            )
            # Consume delimiter
            try:
                d_tag = _read_tag(stream, little_endian)
                if d_tag == ITEM_DELIMITATION:
                    stream.read(4)
            except Exception:
                pass
            items.append(item_elements)
        elif length == 0:
            items.append([])
        else:
            item_data = stream.read(length)
            item_stream = BytesIO(item_data)
            item_elements = _read_data_elements(
                item_stream, explicit_vr, little_endian,
            )
            items.append(item_elements)

    return items
