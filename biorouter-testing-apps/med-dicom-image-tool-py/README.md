# medicom — Pure-Python DICOM Medical Image Toolkit

A minimal, zero-dependency DICOM Part-10 reader, image processor, and exporter
written entirely in standard-library Python.

## Features

- **Pure-Python DICOM reader** — parses Part-10 binary format (preamble, DICM
  magic, file meta, data elements with explicit & implicit VR, nested sequences).
- **Tag extraction** — patient, study, series, instance UIDs; modality; pixel
  geometry (rows/cols, bits, pixel spacing); display parameters (window
  center/width, rescale slope/intercept).
- **Image operations** — windowing/leveling to 8-bit, CT Hounsfield-unit
  rescale, basic intensity statistics, simple thresholding/segmentation,
  histogram computation.
- **Series loader** — groups instances by series, sorts by image position
  patient / instance number.
- **Pure-Python PNG / PGM writer** — no PIL / Pillow needed.
- **Synthetic DICOM generator** — produces valid minimal DICOM files for
  testing without any real patient data.
- **CLI** — read a DICOM file (or a synthetic one), print a header summary,
  and write a windowed image.

## Quick start

```bash
pip install -e .
medicom --help

# Generate a synthetic CT phantom and window it
python -m medicom.generate --output phantom.dcm
medicom phantom.dcm --output phantom.png

# Run the test suite
pytest
```

## Project layout

```
src/medicom/
    __init__.py
    dicom/          # low-level DICOM reader
        __init__.py
        vr.py       # value-representation definitions
        tags.py     # tag constants and lookup helpers
        reader.py   # Part-10 binary parser
    image.py        # windowing, HU rescale, segmentation, stats
    series.py       # instance grouping and sorting
    writer.py       # PNG and PGM pure-Python writers
    generate.py     # synthetic DICOM file generator
    cli.py          # command-line interface

tests/
    test_reader.py
    test_tags.py
    test_image.py
    test_series.py
    test_writer.py
    test_generate.py
    test_cli.py
```

## License

MIT
