"""DICOM tag constants and lookup tables.

Tags are 32-bit unsigned integers encoded as (group, element) → 0xGGGGEEEE.
This module provides commonly-used tag constants and a name/keyword lookup.
"""

from __future__ import annotations
from dataclasses import dataclass
from typing import Dict, Optional, Tuple


@dataclass(frozen=True)
class TagInfo:
    group: int
    element: int
    tag_hex: str
    keyword: str
    name: Optional[str]
    vr: Optional[str]  # default VR (may be overridden in dataset)


# ── Master tag table ─────────────────────────────────────────────────────────

TAGS: Dict[Tag, 'TagInfo'] = {}


@dataclass(frozen=True)
class Tag:
    """A DICOM tag identifier."""
    group: int
    element: int

    @property
    def value(self) -> int:
        return (self.group << 16) | self.element

    @property
    def hex(self) -> str:
        return f"({self.group:04X},{self.element:04X})"

    @property
    def keyword(self) -> str:
        """Return the DICOM keyword for this tag, or the hex string."""
        info = TAGS.get(self)
        if info is not None:
            return info.keyword
        return self.hex

    @classmethod
    def from_hex(cls, s: str) -> "Tag":
        """Parse '(GGGG,EEEE)' or 'GGGGEEEE' or 'GGGG,EEEE'."""
        s = s.strip().strip("()")
        parts = s.replace(",", " ").split()
        g = int(parts[0], 16)
        e = int(parts[1], 16)
        return cls(g, e)

    def __eq__(self, other):
        if isinstance(other, Tag):
            return self.group == other.group and self.element == other.element
        if isinstance(other, tuple) and len(other) == 2:
            return self.group == other[0] and self.element == other[1]
        return NotImplemented

    def __hash__(self):
        return hash((self.group, self.element))

    def __repr__(self):
        return f"Tag({self.group:#06x}, {self.element:#06x})"


def _t(g: int, e: int, kw: str, name: str, vr: Optional[str] = None) -> Tag:
    tag = Tag(g, e)
    TAGS[tag] = TagInfo(g, e, f"({g:04X},{e:04X})", kw, name, vr)
    return tag


# File Meta Information Group (0002,xxxx)
FILE_META_INFO_VERSION       = _t(0x0002, 0x0001, "FileMetaInformationVersion",       "File Meta Information Version",         "OB")
MEDIA_STORAGE_SOP_CLASS_UID  = _t(0x0002, 0x0002, "MediaStorageSOPClassUID",          "Media Storage SOP Class UID",           "UI")
MEDIA_STORAGE_SOP_INST_UID   = _t(0x0002, 0x0003, "MediaStorageSOPInstanceUID",       "Media Storage SOP Instance UID",        "UI")
TRANSFER_SYNTAX_UID          = _t(0x0002, 0x0010, "TransferSyntaxUID",                "Transfer Syntax UID",                  "UI")
IMPLEMENTATION_CLASS_UID     = _t(0x0002, 0x0012, "ImplementationClassUID",           "Implementation Class UID",             "UI")
IMPLEMENTATION_VERSION_NAME  = _t(0x0002, 0x0013, "ImplementationVersionName",        "Implementation Version Name",          "SH")
SPECIFIC_CHARACTER_SET       = _t(0x0008, 0x0005, "SpecificCharacterSet",             "Specific Character Set",               "CS")

# Patient module
PATIENT_ID                   = _t(0x0010, 0x0020, "PatientID",                       "Patient ID",                          "LO")
PATIENT_NAME                 = _t(0x0010, 0x0010, "PatientName",                     "Patient Name",                        "PN")
PATIENT_BIRTH_DATE           = _t(0x0010, 0x0030, "PatientBirthDate",                "Patient's Birth Date",                 "DA")
PATIENT_SEX                  = _t(0x0010, 0x0040, "PatientSex",                      "Patient's Sex",                       "CS")

# General Study module
STUDY_INSTANCE_UID           = _t(0x0020, 0x000D, "StudyInstanceUID",                "Study Instance UID",                  "UI")
STUDY_DATE                   = _t(0x0008, 0x0020, "StudyDate",                       "Study Date",                          "DA")
STUDY_TIME                   = _t(0x0008, 0x0030, "StudyTime",                       "Study Time",                          "TM")
STUDY_ID                     = _t(0x0020, 0x0010, "StudyID",                         "Study ID",                            "SH")
STUDY_DESCRIPTION            = _t(0x0008, 0x1030, "StudyDescription",                "Study Description",                   "LO")
ACCESSION_NUMBER             = _t(0x0008, 0x0050, "AccessionNumber",                 "Accession Number",                    "SH")
REFERRING_PHYSICIAN_NAME     = _t(0x0008, 0x0090, "ReferringPhysicianName",          "Referring Physician's Name",          "PN")

# General Series module
SERIES_INSTANCE_UID          = _t(0x0020, 0x000E, "SeriesInstanceUID",               "Series Instance UID",                 "UI")
SERIES_NUMBER                = _t(0x0020, 0x0011, "SeriesNumber",                    "Series Number",                        "IS")
MODALITY                     = _t(0x0008, 0x0060, "Modality",                        "Modality",                            "CS")
SERIES_DESCRIPTION           = _t(0x0008, 0x103E, "SeriesDescription",               "Series Description",                  "LO")
BODY_PART_EXAMINED           = _t(0x0018, 0x0015, "BodyPartExamined",                "Body Part Examined",                  "CS")

# General Image module
INSTANCE_NUMBER              = _t(0x0020, 0x0013, "InstanceNumber",                   "Instance Number",                      "IS")
CONTENT_DATE                 = _t(0x0008, 0x0023, "ContentDate",                      "Content Date",                        "DA")
CONTENT_TIME                 = _t(0x0008, 0x0033, "ContentTime",                      "Content Time",                        "TM")
IMAGE_TYPE                   = _t(0x0008, 0x0008, "ImageType",                       "Image Type",                          "CS")
ACQUISITION_NUMBER           = _t(0x0020, 0x0012, "AcquisitionNumber",               "Acquisition Number",                   "IS")
ACQUISITION_DATE             = _t(0x0008, 0x0022, "AcquisitionDate",                 "Acquisition Date",                    "DA")
ACQUISITION_TIME             = _t(0x0008, 0x0032, "AcquisitionTime",                 "Acquisition Time",                    "TM")

# Image Plane module
PIXEL_SPACING                = _t(0x0028, 0x0030, "PixelSpacing",                    "Pixel Spacing",                       "DS")
IMAGE_POSITION_PATIENT       = _t(0x0020, 0x0032, "ImagePositionPatient",            "Image Position Patient",              "DS")
IMAGE_ORIENTATION_PATIENT    = _t(0x0020, 0x0037, "ImageOrientationPatient",         "Image Orientation Patient",           "DS")
SLICE_LOCATION               = _t(0x0020, 0x1041, "SliceLocation",                   "Slice Location",                      "DS")

# Image Pixel module
ROWS                         = _t(0x0028, 0x0010, "Rows",                            "Rows",                                "US")
COLUMNS                      = _t(0x0028, 0x0011, "Columns",                         "Columns",                             "US")
BITS_ALLOCATED               = _t(0x0028, 0x0100, "BitsAllocated",                   "Bits Allocated",                      "US")
BITS_STORED                  = _t(0x0028, 0x0101, "BitsStored",                      "Bits Stored",                         "US")
HIGH_BIT                     = _t(0x0028, 0x0102, "HighBit",                         "High Bit",                            "US")
PIXEL_REPRESENTATION         = _t(0x0028, 0x0103, "PixelRepresentation",             "Pixel Representation",                "US")
NUMBER_OF_FRAMES             = _t(0x0028, 0x0008, "NumberOfFrames",                  "Number of Frames",                    "IS")
PLANAR_CONFIGURATION         = _t(0x0028, 0x0006, "PlanarConfiguration",             "Planar Configuration",                "US")
SAMPLES_PER_PIXEL            = _t(0x0028, 0x0002, "SamplesPerPixel",                 "Samples Per Pixel",                   "US")
PHOTOMETRIC_INTERPRETATION   = _t(0x0028, 0x0004, "PhotometricInterpretation",       "Photometric Interpretation",          "CS")

# VOI LUT (display) module
WINDOW_CENTER                = _t(0x0028, 0x1050, "WindowCenter",                    "Window Center",                       "DS")
WINDOW_WIDTH                 = _t(0x0028, 0x1051, "WindowWidth",                     "Window Width",                        "DS")
WINDOW_CENTER_WIDTH_EXPL     = _t(0x0028, 0x1055, "WindowCenterWidthExplanation",    "Window Center / Width Explanation",   "LO")
VOI_LUT_FUNCTION             = _t(0x0028, 0x1056, "VOILUTFunction",                  "VOI LUT Function",                    "CS")

# Rescale module (CT etc.)
RESCALE_INTERCEPT            = _t(0x0028, 0x1052, "RescaleIntercept",                "Rescale Intercept",                   "DS")
RESCALE_SLOPE                = _t(0x0028, 0x1053, "RescaleSlope",                    "Rescale Slope",                       "DS")
RESCALE_TYPE                 = _t(0x0028, 0x1054, "RescaleType",                     "Rescale Type",                        "LS")

# SOP Common module
SOP_CLASS_UID                = _t(0x0008, 0x0016, "SOPClassUID",                     "SOP Class UID",                       "UI")
SOP_INSTANCE_UID             = _t(0x0008, 0x0018, "SOPInstanceUID",                  "SOP Instance UID",                    "UI")

# Pixel Data
PIXEL_DATA                   = _t(0x7FE0, 0x0010, "PixelData",                       "Pixel Data",                          "OW")

# Sequence tags
SHARED_FUNCTIONAL_GROUPS_SEQUENCE = _t(0x5200, 0x9229, "SharedFunctionalGroupsSequence",
                                        "Shared Functional Groups Sequence",   "SQ")
PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE = _t(0x5200, 0x9230, "PerFrameFunctionalGroupsSequence",
                                           "Per-Frame Functional Groups Sequence", "SQ")
FRAME_CONTENT_SEQUENCE       = _t(0x0020, 0x9111, "FrameContentSequence",
                                   "Frame Content Sequence",                  "SQ")
PLANE_POSITION_SEQUENCE      = _t(0x0020, 0x9113, "PlanePositionSequence",
                                   "Plane Position Sequence",                 "SQ")
PLANE_ORIENTATION_SEQUENCE   = _t(0x0020, 0x9116, "PlaneOrientationSequence",
                                   "Plane Orientation Sequence",              "SQ")
PIXEL_MEASUREMENT_SEQUENCE   = _t(0x0028, 0x9110, "PixelMeasuresSequence",
                                   "Pixel Measures Sequence",                 "SQ")
WINDOW_VALUE_SEQUENCE        = _t(0x0028, 0x9132, "ROIValueSequence",
                                   "ROI Value Sequence",                      "SQ")
RESCALE_FUNCTION_GROUP_SEQUENCE = _t(0x0028, 0x9145, "RescaleFunctionGroupSequence",
                                      "Rescale Function Group Sequence",       "SQ")

# Sequence item delimiters
ITEM                         = Tag(0xFFFE, 0xE000)
ITEM_DELIMITATION            = Tag(0xFFFE, 0xE00D)
SEQUENCE_DELIMITATION        = Tag(0xFFFE, 0xDDFF)


# ── Convenience helpers ──────────────────────────────────────────────────────

def tag_by_keyword(keyword: str) -> Optional[Tag]:
    """Look up a tag by its DICOM keyword."""
    for tag, info in TAGS.items():
        if info.keyword == keyword:
            return tag
    return None


def tag_by_hex(hex_str: str) -> Optional[Tag]:
    """Look up a tag by its hex string e.g. '(0010,0020)'."""
    t = Tag.from_hex(hex_str)
    return t if t in TAGS else t
