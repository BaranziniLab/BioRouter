"""Command-line interface for medicom.

Usage:
  medicom <dicom_file>                    # Print header summary
  medicom <dicom_file> -o output.png     # Window and write image
  medicom <dir> --series                  # Load and summarize series
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from medicom.dicom.reader import DICOMFile
from medicom.dicom.tags import (
    Tag,
    ROWS, COLUMNS, BITS_ALLOCATED, BITS_STORED,
    WINDOW_CENTER, WINDOW_WIDTH,
    RESCALE_SLOPE, RESCALE_INTERCEPT,
    PIXEL_DATA,
)
from medicom.image import apply_window, window_width_height_to_8bit
from medicom.writer import write_png, write_pgm


def _parse_ds_list(value: str) -> list:
    """Parse a DICOM Decimal String that may contain backslash-separated values."""
    parts = value.replace("\\", " ").split()
    try:
        return [float(p) for p in parts]
    except ValueError:
        return [value]


def cmd_read(args):
    """Read a DICOM file and print header summary."""
    try:
        dcm = DICOMFile.from_path(args.input)
        print(dcm.summary())
    except Exception as e:
        print(f"Error reading DICOM file: {e}", file=sys.stderr)
        sys.exit(1)


def cmd_window(args):
    """Read a DICOM file, apply windowing, and write output image."""
    try:
        dcm = DICOMFile.from_path(args.input)
    except Exception as e:
        print(f"Error reading DICOM file: {e}", file=sys.stderr)
        sys.exit(1)

    if not dcm.has_pixel_data():
        print("Error: no pixel data found in DICOM file", file=sys.stderr)
        sys.exit(1)

    # Get dimensions and window parameters
    rows = dcm.dataset.get_int(ROWS, 0)
    cols = dcm.dataset.get_int(COLUMNS, 0)

    if args.window_center is not None and args.window_width is not None:
        wc = args.window_center
        ww = args.window_width
    else:
        # Try to read from DICOM tags
        wc_raw = dcm.dataset.get_str(WINDOW_CENTER, "")
        ww_raw = dcm.dataset.get_str(WINDOW_WIDTH, "")
        if wc_raw and ww_raw:
            wc_vals = _parse_ds_list(wc_raw)
            ww_vals = _parse_ds_list(ww_raw)
            wc = float(wc_vals[0]) if wc_vals else 40.0
            ww = float(ww_vals[0]) if ww_vals else 400.0
        else:
            wc = 40.0
            ww = 400.0
            print(f"Note: No window center/width found; using defaults (WC={wc}, WW={ww})")

    slope = dcm.dataset.get_float(RESCALE_SLOPE, 1.0)
    intercept = dcm.dataset.get_float(RESCALE_INTERCEPT, 0.0)
    bits_stored = dcm.dataset.get_int(BITS_STORED, 12)
    pixel_rep = dcm.dataset.get_int(Tag(0x0028, 0x0103), 0)

    # Get raw pixels
    raw_pixels = dcm.pixel_array()

    # Apply windowing
    windowed = window_width_height_to_8bit(
        raw_pixels,
        window_center=wc,
        window_width=ww,
        slope=slope,
        intercept=intercept,
        bits_stored=bits_stored,
        pixel_representation=pixel_rep,
    )

    # Write output
    output = Path(args.output)
    if output.suffix.lower() == ".pgm":
        write_pgm(windowed, cols, rows, output)
    else:
        write_png(windowed, cols, rows, output)

    print(f"Written: {output} ({cols}x{rows}, WC={wc}, WW={ww})")

    # Also print summary
    print()
    print(dcm.summary())


def cmd_info(args):
    """Print only the header summary (alias for read)."""
    cmd_read(args)


def cmd_generate(args):
    """Generate a synthetic DICOM file."""
    from medicom.generate import generate_dicom

    output = generate_dicom(
        output=args.output,
        rows=args.rows,
        cols=args.cols,
        modality=args.modality,
        patient_name=args.patient_name,
        patient_id=args.patient_id,
        pixel_pattern=args.pattern,
        rescale_slope=args.rescale_slope,
        rescale_intercept=args.rescale_intercept,
        window_center=args.window_center,
        window_width=args.window_width,
    )
    print(f"Generated: {output} ({args.rows}x{args.cols}, {args.modality}, pattern={args.pattern})")


def main(argv=None):
    """Main entry point for the medicom CLI."""
    parser = argparse.ArgumentParser(
        prog="medicom",
        description="Pure-Python DICOM Medical Image Toolkit",
    )
    subparsers = parser.add_subparsers(dest="command")

    # ── read / info ──────────────────────────────────────────────────────
    read_parser = subparsers.add_parser("read", help="Read DICOM file and print header")
    read_parser.add_argument("input", help="DICOM file path")
    read_parser.set_defaults(func=cmd_read)

    info_parser = subparsers.add_parser("info", help="Print DICOM header summary")
    info_parser.add_argument("input", help="DICOM file path")
    info_parser.set_defaults(func=cmd_info)

    # ── window ───────────────────────────────────────────────────────────
    window_parser = subparsers.add_parser("window", help="Apply windowing and write image")
    window_parser.add_argument("input", help="DICOM file path")
    window_parser.add_argument("-o", "--output", required=True, help="Output image path (.png or .pgm)")
    window_parser.add_argument("--window-center", type=float, default=None, help="Window center (WC)")
    window_parser.add_argument("--window-width", type=float, default=None, help="Window width (WW)")
    window_parser.set_defaults(func=cmd_window)

    # ── generate ─────────────────────────────────────────────────────────
    gen_parser = subparsers.add_parser("generate", help="Generate a synthetic DICOM file")
    gen_parser.add_argument("-o", "--output", default="synthetic.dcm", help="Output DICOM file path")
    gen_parser.add_argument("--rows", type=int, default=64, help="Image rows")
    gen_parser.add_argument("--cols", type=int, default=64, help="Image columns")
    gen_parser.add_argument("--modality", default="CT", help="Modality (CT, MR, XR)")
    gen_parser.add_argument("--patient-name", default="Synthetic^Patient", help="Patient name")
    gen_parser.add_argument("--patient-id", default="SYNTH001", help="Patient ID")
    gen_parser.add_argument("--pattern", default="circle",
                            choices=["circle", "steps", "gradient", "checker", "uniform"],
                            help="Phantom pattern")
    gen_parser.add_argument("--rescale-slope", type=float, default=1.0, help="Rescale slope")
    gen_parser.add_argument("--rescale-intercept", type=float, default=-1024.0, help="Rescale intercept")
    gen_parser.add_argument("--window-center", type=float, default=40.0, help="Window center")
    gen_parser.add_argument("--window-width", type=float, default=400.0, help="Window width")
    gen_parser.set_defaults(func=cmd_generate)

    # ── parse and dispatch ───────────────────────────────────────────────
    if argv is None:
        args = parser.parse_args()
    else:
        args = parser.parse_args(argv)

    if not hasattr(args, "func"):
        parser.print_help()
        sys.exit(0)

    args.func(args)


if __name__ == "__main__":
    main()
