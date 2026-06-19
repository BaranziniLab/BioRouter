"""VCF 4.2 output writer.

Writes variant calls in VCF format with header, sample columns, and
INFO fields including depth, allele frequency, ts/tv, and allele balance.
"""

from __future__ import annotations

import datetime
from io import StringIO
from typing import List, Optional, TextIO

from .models import Variant, VariantType


# ---------------------------------------------------------------------------
# VCF header constants
# ---------------------------------------------------------------------------

_VCF_VERSION = "4.2"

_HEADER_LINES = [
    '##fileformat=VCFv4.2',
    '##source=bio_variant_caller',
    '##INFO=<ID=DP,Number=1,Type=Integer,Description="Total Depth">',
    '##INFO=<ID=AF,Number=A,Type=Float,Description="Allele Frequency">',
    '##INFO=<ID=TSTV,Number=1,Type=String,Description="Transition/Transversion">',
    '##INFO=<ID=AB,Number=A,Type=Float,Description="Allele Balance">',
    '##INFO=<ID=SB,Number=1,Type=Float,Description="Strand Balance (fwd/total)">',
    '##FORMAT=<ID=GT,Number=1,Type=String,Description="Genotype">',
    '##FORMAT=<ID=GQ,Number=1,Type=Integer,Description="Genotype Quality">',
    '##FORMAT=<ID=DP,Number=1,Type=Integer,Description="Read Depth">',
    '##FORMAT=<ID=AD,Number=R,Type=Integer,Description="Allelic Depths">',
]


# ---------------------------------------------------------------------------
# VCF Writer
# ---------------------------------------------------------------------------

class VCFWriter:
    """Write variants in VCF format.

    Parameters
    ----------
    sample_name : str
        Name for the sample column (default "SAMPLE").
    reference_name : str
        Reference name for header (default "ref").
    """

    def __init__(
        self,
        sample_name: str = "SAMPLE",
        reference_name: str = "ref",
    ) -> None:
        self.sample_name = sample_name
        self.reference_name = reference_name

    def write_header(self, out: TextIO) -> None:
        """Write VCF header lines."""
        for line in _HEADER_LINES:
            out.write(line + "\n")
        out.write(
            f'##reference=<ID={self.reference_name},'
            f'Length=0,Source=custom>\n'
        )
        out.write(
            f'#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT'
            f'\t{self.sample_name}\n'
        )

    def write_variant(self, v: Variant, out: TextIO) -> None:
        """Write a single variant record."""
        chrom = v.chrom
        pos = v.pos + 1  # VCF is 1-based
        var_id = "."
        ref = v.ref
        alt = v.alt
        qual = f"{v.quality:.1f}" if v.quality > 0 else "."
        filt = self._filter_field(v)

        # INFO field
        info_parts = [f"DP={v.depth}"]
        if v.allele_frequency is not None:
            info_parts.append(f"AF={v.allele_frequency:.4f}")
        if v.ts_tv:
            info_parts.append(f"TSTV={v.ts_tv}")
        if v.allele_balance is not None:
            info_parts.append(f"AB={v.allele_balance:.4f}")
        if v.strand_balance is not None:
            info_parts.append(f"SB={v.strand_balance:.4f}")
        info = ";".join(info_parts)

        # FORMAT and sample columns
        fmt = "GT:GQ:DP:AD"
        gt_str = v.genotype.value
        gq = int(v.genotype_quality)
        dp = v.depth
        alt_count = v.alt_count
        ref_count = dp - alt_count
        sample = f"{gt_str}:{gq}:{dp}:{ref_count},{alt_count}"

        out.write(
            f"{chrom}\t{pos}\t{var_id}\t{ref}\t{alt}\t{qual}\t"
            f"{filt}\t{info}\t{fmt}\t{sample}\n"
        )

    def write_variants(
        self, variants: List[Variant], out: Optional[TextIO] = None
    ) -> str:
        """Write all variants to a file-like object. Returns the content as string."""
        buf = out or StringIO()
        self.write_header(buf)
        for v in variants:
            self.write_variant(v, buf)
        if out is None:
            return buf.getvalue()  # type: ignore[return-value]
        return ""

    def write_to_file(
        self, variants: List[Variant], filepath: str
    ) -> int:
        """Write VCF to a file. Returns number of variant records written."""
        with open(filepath, "w") as fh:
            self.write_header(fh)
            for v in variants:
                self.write_variant(v, fh)
        return len(variants)

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _filter_field(v: Variant) -> str:
        """Determine FILTER column value."""
        filters = []
        if v.depth < 8:
            filters.append("LowDepth")
        if v.strand_balance is not None and (v.strand_balance < 0.1 or v.strand_balance > 0.9):
            filters.append("StrandBias")
        if v.genotype_quality < 20:
            filters.append("LowGQ")
        return ";".join(filters) if filters else "PASS"


# ---------------------------------------------------------------------------
# Convenience
# ---------------------------------------------------------------------------

def write_vcf(
    variants: List[Variant],
    filepath: str,
    sample_name: str = "SAMPLE",
    reference_name: str = "ref",
) -> int:
    """Write variants to a VCF file. Returns record count."""
    writer = VCFWriter(sample_name=sample_name, reference_name=reference_name)
    return writer.write_to_file(variants, filepath)


def variants_to_vcf_string(
    variants: List[Variant],
    sample_name: str = "SAMPLE",
    reference_name: str = "ref",
) -> str:
    """Return VCF content as a string."""
    writer = VCFWriter(sample_name=sample_name, reference_name=reference_name)
    return writer.write_variants(variants)
