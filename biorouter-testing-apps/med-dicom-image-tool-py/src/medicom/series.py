"""Series loader — groups DICOM instances and sorts them.

Loads a directory of DICOM files, groups by SeriesInstanceUID, and sorts
instances within each series by ImagePositionPatient (Z-coordinate),
InstanceNumber, or SliceLocation.
"""

from __future__ import annotations

from pathlib import Path
from typing import Dict, List, Optional, Tuple

from medicom.dicom.reader import DICOMFile
from medicom.dicom.tags import (
    Tag,
    SERIES_INSTANCE_UID,
    INSTANCE_NUMBER,
    IMAGE_POSITION_PATIENT,
    SLICE_LOCATION,
    ROWS,
    COLUMNS,
    MODALITY,
)


class DICOMInstance:
    """Wrapper around a loaded DICOM file with metadata for sorting."""

    def __init__(self, dcm: DICOMFile, path: Path):
        self.dcm = dcm
        self.path = path
        self.series_uid: str = dcm.dataset.get_str(SERIES_INSTANCE_UID, "")
        self.instance_number: int = dcm.dataset.get_int(INSTANCE_NUMBER, 0)
        self.slice_location: float = dcm.dataset.get_float(SLICE_LOCATION, 0.0)
        self.image_position_z: float = self._parse_position_z()
        self.rows: int = dcm.dataset.get_int(ROWS, 0)
        self.cols: int = dcm.dataset.get_int(COLUMNS, 0)

    def _parse_position_z(self) -> float:
        """Extract Z component from ImagePositionPatient (DS string)."""
        raw = self.dcm.dataset.get_str(IMAGE_POSITION_PATIENT, "")
        if not raw:
            return 0.0
        parts = raw.replace("\\", " ").split()
        try:
            return float(parts[2]) if len(parts) >= 3 else 0.0
        except (ValueError, IndexError):
            return 0.0


class DICOMSeries:
    """A sorted series of DICOM instances."""

    def __init__(self, series_uid: str, instances: List[DICOMInstance]):
        self.series_uid = series_uid
        self.instances = instances
        self.modality: str = instances[0].dcm.dataset.get_str(MODALITY, "") if instances else ""
        self.rows: int = instances[0].rows if instances else 0
        self.cols: int = instances[0].cols if instances else 0

    def __len__(self):
        return len(self.instances)

    def __iter__(self):
        return iter(self.instances)

    def __getitem__(self, idx):
        return self.instances[idx]


def load_series(
    path: Union[str, Path],
    sort_by: str = "position",
) -> Dict[str, DICOMSeries]:
    """Load DICOM files from a directory and group by series.

    Parameters
    ----------
    path : directory containing DICOM files (searched recursively)
    sort_by : "position" (ImagePositionPatient Z), "instance" (InstanceNumber),
              or "location" (SliceLocation)

    Returns
    -------
    Dict mapping SeriesInstanceUID → DICOMSeries
    """
    path = Path(path)

    # Find all DICOM files (try parsing each — reject non-DICOM)
    instances: List[DICOMInstance] = []

    if path.is_file():
        # Single file
        try:
            dcm = DICOMFile.from_path(path)
            instances.append(DICOMInstance(dcm, path))
        except Exception:
            return {}

    elif path.is_dir():
        # Recurse into directory
        for dcm_path in sorted(path.rglob("*.dcm")):
            try:
                dcm = DICOMFile.from_path(dcm_path)
                instances.append(DICOMInstance(dcm, dcm_path))
            except Exception:
                continue  # skip non-DICOM files
    else:
        raise FileNotFoundError(f"Path not found: {path}")

    # Group by series UID
    groups: Dict[str, List[DICOMInstance]] = {}
    for inst in instances:
        uid = inst.series_uid or "unknown"
        groups.setdefault(uid, []).append(inst)

    # Sort each series
    series_map: Dict[str, DICOMSeries] = {}
    for uid, inst_list in groups.items():
        sorted_instances = _sort_instances(inst_list, sort_by)
        series_map[uid] = DICOMSeries(uid, sorted_instances)

    return series_map


def load_single_series(
    path: Union[str, Path],
    sort_by: str = "position",
    series_uid: Optional[str] = None,
) -> DICOMSeries:
    """Load a single series from a directory.

    If the directory contains multiple series, returns the first one
    (or the one matching *series_uid*).
    """
    series_map = load_series(path, sort_by)

    if not series_map:
        raise ValueError(f"No DICOM files found in {path}")

    if series_uid and series_uid in series_map:
        return series_map[series_uid]

    # Return first (or only) series
    return next(iter(series_map.values()))


def sort_instances(
    instances: List[DICOMInstance],
    sort_by: str = "position",
) -> List[DICOMInstance]:
    """Public sorting function."""
    return _sort_instances(instances, sort_by)


def _sort_instances(
    instances: List[DICOMInstance],
    sort_by: str,
) -> List[DICOMInstance]:
    """Sort instances by the given criterion."""
    if sort_by == "position":
        return sorted(instances, key=lambda i: i.image_position_z)
    elif sort_by == "instance":
        return sorted(instances, key=lambda i: i.instance_number)
    elif sort_by == "location":
        return sorted(instances, key=lambda i: i.slice_location)
    else:
        return sorted(instances, key=lambda i: i.instance_number)


def get_series_pixel_stack(
    series: DICOMSeries,
) -> List[List[int]]:
    """Extract pixel arrays for all instances in a series, in sorted order.

    Returns a list of 1D pixel arrays (one per slice).
    """
    stacks = []
    for inst in series:
        try:
            pixel_bytes = inst.dcm.pixel_array()
            count = len(pixel_bytes) // 2
            import struct
            pixels = list(struct.unpack(f"<{count}H", pixel_bytes))
            stacks.append(pixels)
        except Exception:
            stacks.append([])
    return stacks


# Import for type annotation
from typing import Union
