"""medicom.dicom — Low-level DICOM Part-10 parsing."""

from medicom.dicom.reader import DICOMFile
from medicom.dicom.tags import Tag, TAGS

__all__ = ["DICOMFile", "Tag", "TAGS"]
