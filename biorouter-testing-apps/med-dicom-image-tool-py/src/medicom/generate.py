"""Synthetic DICOM file generator for testing.

Produces valid minimal DICOM Part-10 files with:
  - Standard preamble + DICM magic
  - File Meta Information (explicit VR LE, Transfer Syntax = Explicit VR LE)
  - Patient, Study, Series, Instance, Image pixel modules
  - Configurable modality (CT, MR, XR, etc.)
  - Programmable pixel data with optional phantom patterns
  - Nested sequences (SharedFunctionalGroups with pixel measures)

No external dependencies — writes binary DICOM from scratch.
"""

from __future__ import annotations

import struct
import time
from pathlib import Path
from typing import List, Optional, Tuple, Union
import random


def _ui_bytes(value: str) -> bytes:
    """Encode a UI value (pad with 0x00 to even length)."""
    b = value.encode("ascii")
    if len(b) % 2 != 0:
        b += b"\x00"
    return b


def _cs_bytes(value: str) -> bytes:
    """Encode a CS value (pad with spaces to even length)."""
    b = value.encode("ascii")
    if len(b) % 2 != 0:
        b += b" "
    return b


def _lo_bytes(value: str) -> bytes:
    """Encode an LO value (pad with spaces to even length)."""
    b = value.encode("ascii")
    if len(b) % 2 != 0:
        b += b" "
    return b


def _ds_bytes(value: str) -> bytes:
    """Encode a DS value (pad with spaces to even length)."""
    b = value.encode("ascii")
    if len(b) % 2 != 0:
        b += b" "
    return b


def _da_bytes(value: str) -> bytes:
    """Encode a DA value (8 bytes, YYYYMMDD)."""
    return value.encode("ascii")


def _tm_bytes(value: str) -> bytes:
    """Encode a TM value (even-length HHMMSS.FFFFFF)."""
    b = value.encode("ascii")
    if len(b) % 2 != 0:
        b += b" "
    return b


def _is_bytes(value: str) -> bytes:
    """Encode an IS value (pad with spaces to even length)."""
    b = value.encode("ascii")
    if len(b) % 2 != 0:
        b += b" "
    return b


def _sh_bytes(value: str) -> bytes:
    """Encode an SH value."""
    b = value.encode("ascii")
    if len(b) % 2 != 0:
        b += b" "
    return b


def _pn_bytes(value: str) -> bytes:
    """Encode a PN value."""
    b = value.encode("ascii")
    if len(b) % 2 != 0:
        b += b" "
    return b


def _write_tag(stream, group: int, element: int):
    stream.write(struct.pack("<HH", group, element))


def _write_vr_explicit(stream, vr: str):
    stream.write(vr.encode("ascii"))


def _write_length_explicit_short(stream, length: int):
    """Write 2-byte length (for short-VR explicit)."""
    stream.write(struct.pack("<H", length))


def _write_length_explicit_long(stream, length: int):
    """Write 4-byte length with 2-byte reserved (for long-VR explicit)."""
    stream.write(b"\x00\x00")
    stream.write(struct.pack("<I", length))


def _write_length_implicit(stream, length: int):
    """Write 4-byte length (for implicit VR)."""
    stream.write(struct.pack("<I", length))


def _write_element_explicit_short(
    stream, group: int, element: int, vr: str, value: bytes
):
    """Write a data element with explicit VR, short-length VRs."""
    _write_tag(stream, group, element)
    _write_vr_explicit(stream, vr)
    _write_length_explicit_short(stream, len(value))
    stream.write(value)


def _write_element_explicit_long(
    stream, group: int, element: int, vr: str, value: bytes
):
    """Write a data element with explicit VR, long-length VRs (OB, OW, SQ, UN)."""
    _write_tag(stream, group, element)
    _write_vr_explicit(stream, vr)
    _write_length_explicit_long(stream, len(value))
    stream.write(value)


def _generate_uid(prefix: str = "1.2.840.113619.2") -> str:
    """Generate a random DICOM UID."""
    root = prefix
    suffix = ".".join(str(random.randint(0, 99999)) for _ in range(4))
    uid = f"{root}.{suffix}"
    # Pad to even length (UIDs must be even number of chars for DICOM)
    if len(uid) % 2 != 0:
        uid += "0"
    return uid


def _generate_phantom_pixels(
    rows: int,
    cols: int,
    bits_stored: int,
    signed: bool,
    pattern: str = "circle",
) -> bytes:
    """Generate synthetic pixel data with known phantom patterns.

    Patterns:
      "circle"  — solid circle (HU=30) in air (HU=-1000)
      "steps"   — horizontal bars of increasing intensity
      "gradient"— smooth gradient from 0 to max
      "checker" — alternating 0/1 blocks
      "uniform" — all pixels same value
    """
    max_val = (2**bits_stored) - 1
    pixels: List[int] = []

    for r in range(rows):
        for c in range(cols):
            if pattern == "circle":
                cy, cx = rows // 2, cols // 2
                radius = min(rows, cols) // 3
                dist = ((r - cy) ** 2 + (c - cx) ** 2) ** 0.5
                val = 30 if dist <= radius else -1000  # HU values
                # Convert HU to stored value (assume slope=1, intercept=-1024)
                val = val + 1024  # undo intercept for storage
                if val < 0:
                    val = 0
                if val > max_val:
                    val = max_val
                pixels.append(val)

            elif pattern == "steps":
                band_width = cols // 5
                band_idx = c // band_width if band_width > 0 else 0
                val = int((band_idx / 4) * max_val)
                pixels.append(min(val, max_val))

            elif pattern == "gradient":
                val = int((r * cols + c) / (rows * cols) * max_val)
                pixels.append(val)

            elif pattern == "checker":
                block = 8
                val = max_val if ((r // block) + (c // block)) % 2 == 0 else 0
                pixels.append(val)

            elif pattern == "uniform":
                pixels.append(max_val // 2)

            else:
                pixels.append(0)

    return struct.pack(f"<{len(pixels)}H", *pixels)


def generate_dicom(
    output: Union[str, Path],
    rows: int = 64,
    cols: int = 64,
    bits_allocated: int = 16,
    bits_stored: int = 12,
    high_bit: int = 11,
    pixel_representation: int = 0,  # unsigned
    modality: str = "CT",
    patient_name: str = "Synthetic^Patient",
    patient_id: str = "SYNTH001",
    study_uid: Optional[str] = None,
    series_uid: Optional[str] = None,
    instance_uid: Optional[str] = None,
    instance_number: int = 1,
    rescale_slope: float = 1.0,
    rescale_intercept: float = -1024.0,
    window_center: float = 40.0,
    window_width: float = 400.0,
    pixel_spacing: str = "0.5\\0.5",
    image_position: str = "0.0\\0.0\\0.0",
    image_orientation: str = "1.0\\0.0\\0.0\\0.0\\1.0\\0.0",
    body_part: str = "HEAD",
    study_date: Optional[str] = None,
    study_time: Optional[str] = None,
    series_number: int = 1,
    pixel_pattern: str = "circle",
    transfer_syntax_uid: str = "1.2.840.10008.1.2.1",
    sop_class_uid: str = "1.2.840.10008.5.1.4.1.1.2",  # CT Image Storage
) -> Path:
    """Generate a valid minimal DICOM file.

    Returns the path to the generated file.
    """
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)

    study_uid = study_uid or _generate_uid()
    series_uid = series_uid or _generate_uid()
    instance_uid = instance_uid or _generate_uid()

    now_date = study_date or time.strftime("%Y%m%d")
    now_time = study_time or time.strftime("%H%M%S")

    with open(output, "wb") as f:
        # ── 1. Preamble (128 bytes) ──────────────────────────────────────
        f.write(b"\x00" * 128)

        # ── 2. DICM magic ────────────────────────────────────────────────
        f.write(b"DICM")

        # ── 3. File Meta Information (Explicit VR LE) ────────────────────
        # Meta Information Group Length — compute total meta size first
        import io
        meta_stream = io.BytesIO()

        # Helper for meta writing (same long-format rules as dataset)
        def _wm(group, element, vr, value):
            if vr in ("OB", "OW", "SQ", "UN", "OF", "OD", "OL", "UC", "UR", "OV"):
                _write_element_explicit_long(meta_stream, group, element, vr, value)
            else:
                _write_element_explicit_short(meta_stream, group, element, vr, value)

        _wm(0x0002, 0x0001, "OB", b"\x00\x01")
        _wm(0x0002, 0x0010, "UI", _ui_bytes(transfer_syntax_uid))
        _wm(0x0002, 0x0002, "UI", _ui_bytes(sop_class_uid))
        _wm(0x0002, 0x0003, "UI", _ui_bytes(instance_uid))
        _wm(0x0002, 0x0012, "UI", _ui_bytes("1.2.840.113619.6.374"))
        _wm(0x0002, 0x0013, "SH", _sh_bytes("medicom_test"))

        meta_bytes = meta_stream.getvalue()

        # Write meta group length element first (UL uses short explicit format)
        _write_tag(f, 0x0002, 0x0000)
        _write_vr_explicit(f, "UL")
        _write_length_explicit_short(f, len(meta_bytes))
        f.write(meta_bytes)

        # ── 4. Dataset (Explicit VR LE) ──────────────────────────────────

        # Helper to write elements with explicit VR
        def w(group, element, vr, value):
            if vr in ("OB", "OW", "SQ", "UN", "OF", "OD", "OL", "UC", "UR"):
                _write_element_explicit_long(f, group, element, vr, value)
            else:
                _write_element_explicit_short(f, group, element, vr, value)

        # Specific Character Set
        w(0x0008, 0x0005, "CS", _cs_bytes("ISO_IR 100"))

        # SOP Common
        w(0x0008, 0x0016, "UI", _ui_bytes(sop_class_uid))
        w(0x0008, 0x0018, "UI", _ui_bytes(instance_uid))

        # Image Type
        w(0x0008, 0x0008, "CS", _cs_bytes("ORIGINAL\\PRIMARY\\AXIAL"))

        # Study / Series / Instance
        w(0x0008, 0x0020, "DA", _da_bytes(now_date))
        w(0x0008, 0x0030, "TM", _tm_bytes(now_time))
        w(0x0008, 0x0060, "CS", _cs_bytes(modality))
        w(0x0008, 0x0050, "SH", _sh_bytes("SYN001"))
        w(0x0008, 0x1030, "LO", _lo_bytes("Synthetic Study"))
        w(0x0008, 0x103E, "LO", _lo_bytes("Synthetic Series"))
        w(0x0008, 0x0015, "CS", _cs_bytes(body_part))

        # Patient
        w(0x0010, 0x0010, "PN", _pn_bytes(patient_name))
        w(0x0010, 0x0020, "LO", _lo_bytes(patient_id))
        w(0x0010, 0x0030, "DA", _da_bytes("19800101"))
        w(0x0010, 0x0040, "CS", _cs_bytes("O"))

        # Study / Series / Instance UIDs
        w(0x0020, 0x000D, "UI", _ui_bytes(study_uid))
        w(0x0020, 0x000E, "UI", _ui_bytes(series_uid))
        w(0x0020, 0x0011, "IS", _is_bytes(str(series_number)))
        w(0x0020, 0x0013, "IS", _is_bytes(str(instance_number)))

        # Image Plane
        w(0x0028, 0x0030, "DS", _ds_bytes(pixel_spacing))
        w(0x0020, 0x0032, "DS", _ds_bytes(image_position))
        w(0x0020, 0x0037, "DS", _ds_bytes(image_orientation))
        w(0x0020, 0x1041, "DS", _ds_bytes("0.0"))

        # Image Pixel
        w(0x0028, 0x0002, "US", struct.pack("<H", 1))  # SamplesPerPixel
        w(0x0028, 0x0004, "CS", _cs_bytes("MONOCHROME2"))
        w(0x0028, 0x0010, "US", struct.pack("<H", rows))
        w(0x0028, 0x0011, "US", struct.pack("<H", cols))
        w(0x0028, 0x0100, "US", struct.pack("<H", bits_allocated))
        w(0x0028, 0x0101, "US", struct.pack("<H", bits_stored))
        w(0x0028, 0x0102, "US", struct.pack("<H", high_bit))
        w(0x0028, 0x0103, "US", struct.pack("<H", pixel_representation))

        # VOI LUT
        w(0x0028, 0x1050, "DS", _ds_bytes(str(int(window_center))))
        w(0x0028, 0x1051, "DS", _ds_bytes(str(int(window_width))))

        # Rescale (for CT)
        if modality == "CT":
            w(0x0028, 0x1052, "DS", _ds_bytes(str(int(rescale_intercept))))
            w(0x0028, 0x1053, "DS", _ds_bytes(str(int(rescale_slope))))
            w(0x0028, 0x1054, "LS", _lo_bytes("HU"))

        # ── Shared Functional Groups Sequence ─────────────────────────────
        # Build Pixel Measures Sequence
        pixel_measures_item = io.BytesIO()
        _write_element_explicit_short(pixel_measures_item,
            0x0028, 0x0030, "DS", _ds_bytes(pixel_spacing))
        _write_element_explicit_short(pixel_measures_item,
            0x0018, 0x0050, "DS", _ds_bytes("1.0"))  # SliceThickness
        pixel_measures_bytes = pixel_measures_item.getvalue()

        # Build the SQ containing one item
        sq_item_buf = io.BytesIO()
        _write_tag(sq_item_buf, 0xFFFE, 0xE000)  # Item tag
        sq_item_buf.write(struct.pack("<I", len(pixel_measures_bytes)))
        sq_item_buf.write(pixel_measures_bytes)
        sq_bytes = sq_item_buf.getvalue()

        # Write SharedFunctionalGroupsSequence
        _write_element_explicit_long(f, 0x5200, 0x9229, "SQ", sq_bytes)

        # ── Pixel Data ────────────────────────────────────────────────────
        pixel_data = _generate_phantom_pixels(
            rows, cols, bits_stored, pixel_representation == 1,
            pattern=pixel_pattern,
        )
        _write_element_explicit_long(f, 0x7FE0, 0x0010, "OW", pixel_data)

    return output


def generate_synthetic_series(
    output_dir: Union[str, Path],
    num_instances: int = 3,
    rows: int = 32,
    cols: int = 32,
    modality: str = "CT",
    **kwargs,
) -> List[Path]:
    """Generate a series of synthetic DICOM files with the same Study/Series UID.

    Instances are sorted by InstanceNumber and have incrementing Z positions.
    """
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    study_uid = _generate_uid()
    series_uid = _generate_uid()
    paths: List[Path] = []

    for i in range(num_instances):
        z_pos = -10.0 + i * 5.0
        path = output_dir / f"slice_{i+1:04d}.dcm"
        generate_dicom(
            output=path,
            rows=rows,
            cols=cols,
            modality=modality,
            study_uid=study_uid,
            series_uid=series_uid,
            instance_number=i + 1,
            image_position=f"0.0\\0.0\\{z_pos:.1f}",
            pixel_pattern="circle",
            **kwargs,
        )
        paths.append(path)

    return paths
