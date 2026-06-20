"""Tests for tag constants and VR definitions."""

import pytest

from medicom.dicom.tags import Tag, TAGS, TagInfo, tag_by_keyword, tag_by_hex
from medicom.dicom.vr import get_vr, vr_name, VR_TABLE


class TestTag:
    def test_tag_creation(self):
        tag = Tag(0x0010, 0x0010)
        assert tag.group == 0x0010
        assert tag.element == 0x0010
        assert tag.value == 0x00100010
        assert tag.hex == "(0010,0010)"

    def test_tag_from_hex(self):
        tag = Tag.from_hex("(0010,0010)")
        assert tag.group == 0x0010
        assert tag.element == 0x0010

    def test_tag_from_hex_no_parens(self):
        tag = Tag.from_hex("0010,0010")
        assert tag == (0x0010, 0x0010)

    def test_tag_equality(self):
        t1 = Tag(0x0010, 0x0010)
        t2 = Tag(0x0010, 0x0010)
        assert t1 == t2
        assert t1 == (0x0010, 0x0010)

    def test_tag_hash(self):
        t1 = Tag(0x0010, 0x0010)
        t2 = Tag(0x0010, 0x0010)
        assert hash(t1) == hash(t2)
        s = {t1, t2}
        assert len(s) == 1

    def test_keyword_lookup(self):
        tag = Tag(0x0010, 0x0010)
        assert tag.keyword == "PatientName"

    def test_keyword_unknown(self):
        tag = Tag(0x9999, 0x9999)
        kw = tag.keyword
        assert "9999" in kw


class TestTAGS:
    def test_all_expected_tags_exist(self):
        expected = [
            Tag(0x0010, 0x0010),  # PatientName
            Tag(0x0010, 0x0020),  # PatientID
            Tag(0x0008, 0x0060),  # Modality
            Tag(0x0028, 0x0010),  # Rows
            Tag(0x7FE0, 0x0010),  # PixelData
        ]
        for tag in expected:
            assert tag in TAGS, f"Tag {tag.hex} not found in TAGS"

    def test_tag_info_fields(self):
        tag = Tag(0x0010, 0x0010)
        info = TAGS[tag]
        assert info.keyword == "PatientName"
        assert info.vr == "PN"
        assert info.name == "Patient Name"

    def test_tag_by_keyword(self):
        tag = tag_by_keyword("PatientName")
        assert tag is not None
        assert tag.group == 0x0010
        assert tag.element == 0x0010

    def test_tag_by_keyword_missing(self):
        assert tag_by_keyword("NonexistentTag") is None

    def test_tag_by_hex(self):
        tag = tag_by_hex("(0028,0010)")
        assert tag.group == 0x0028
        assert tag.element == 0x0010


class TestVR:
    def test_all_common_vrs_present(self):
        common = ["US", "SS", "UL", "SL", "FL", "FD", "OW", "OB",
                   "LO", "SH", "CS", "DS", "IS", "DA", "TM", "UI", "PN",
                   "SQ", "UN", "UT"]
        for vr in common:
            assert vr in VR_TABLE, f"VR '{vr}' not in table"

    def test_get_vr(self):
        info = get_vr("US")
        assert info.explicit_length == 2
        assert info.numeric is True

    def test_get_vr_unknown(self):
        info = get_vr("XX")
        assert info.explicit_length == -1

    def test_vr_name(self):
        assert vr_name("US") == "Unsigned Short"
        assert vr_name("SQ") == "Sequence"
        assert vr_name("CS") == "Code String"

    def test_numeric_vrs(self):
        numeric = ["US", "SS", "UL", "SL", "FL", "FD"]
        for vr in numeric:
            info = get_vr(vr)
            assert info.numeric is True, f"VR '{vr}' should be numeric"
