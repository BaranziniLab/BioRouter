"""
Unit conversion helpers for clinical risk scores.

Provides lightweight, dependency-free converters between common clinical
measurement units (temperature, pressure, weight, height, volume, lab units).
"""
from __future__ import annotations

from typing import Callable, Dict, Optional, Tuple


# ---------------------------------------------------------------------------
# Registry of conversion factors
# Each entry maps (from_unit, to_unit) -> factor so that to_value = from_value * factor.
# For linear conversions only (offsets handled separately).
# ---------------------------------------------------------------------------

_LINEAR: Dict[Tuple[str, str], float] = {}

# --- Temperature ---
# Celsius <-> Fahrenheit
_LINEAR[("C", "F")] = 9.0 / 5.0  # C to F: * 9/5 + 32 handled as offset
_LINEAR[("F", "C")] = 5.0 / 9.0

# --- Pressure ---
# mmHg <-> kPa  (1 mmHg = 0.133322 kPa)
_LINEAR[("mmHg", "kPa")] = 0.133322
_LINEAR[("kPa", "mmHg")] = 1.0 / 0.133322

# --- Weight ---
_LINEAR[("kg", "lb")] = 2.20462
_LINEAR[("lb", "kg")] = 1.0 / 2.20462
_LINEAR[("kg", "g")] = 1000.0
_LINEAR[("g", "kg")] = 0.001
_LINEAR[("lb", "g")] = 453.592
_LINEAR[("g", "lb")] = 1.0 / 453.592

# --- Height / Length ---
_LINEAR[("cm", "in")] = 1.0 / 2.54
_LINEAR[("in", "cm")] = 2.54
_LINEAR[("cm", "m")] = 0.01
_LINEAR[("m", "cm")] = 100.0
_LINEAR[("m", "mm")] = 1000.0
_LINEAR[("mm", "m")] = 0.001

# --- Volume ---
_LINEAR[("L", "mL")] = 1000.0
_LINEAR[("mL", "L")] = 0.001
_LINEAR[("dL", "L")] = 0.1
_LINEAR[("L", "dL")] = 10.0
_LINEAR[("dL", "mL")] = 100.0
_LINEAR[("mL", "dL")] = 0.01

# --- Creatinine ---
_LINEAR[("mg/dL", "µmol/L")] = 88.4
_LINEAR[("µmol/L", "mg/dL")] = 1.0 / 88.4


def _temperature_offset(value: float, from_unit: str, to_unit: str) -> float:
    """Apply temperature conversions that require an additive offset."""
    if from_unit == "C" and to_unit == "F":
        return value * 9.0 / 5.0 + 32.0
    if from_unit == "F" and to_unit == "C":
        return (value - 32.0) * 5.0 / 9.0
    return value


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

TEMPERATURE_UNITS = {"C", "F", "K"}
PRESSURE_UNITS = {"mmHg", "kPa"}
WEIGHT_UNITS = {"kg", "lb", "g"}
HEIGHT_UNITS = {"cm", "m", "mm", "in"}
VOLUME_UNITS = {"L", "mL", "dL"}
CREATININE_UNITS = {"mg/dL", "µmol/L"}


def convert(value: float, from_unit: str, to_unit: str) -> float:
    """
    Convert *value* from *from_unit* to *to_unit*.

    Raises ``ValueError`` if the conversion pair is unknown.
    """
    if from_unit == to_unit:
        return float(value)

    # Temperature special case (offset)
    if from_unit in TEMPERATURE_UNITS and to_unit in TEMPERATURE_UNITS:
        return _temperature_offset(float(value), from_unit, to_unit)

    key = (from_unit, to_unit)
    if key in _LINEAR:
        return float(value) * _LINEAR[key]

    raise ValueError(f"Unknown conversion: {from_unit!r} -> {to_unit!r}")


def to_celsius(value: float, from_unit: str) -> float:
    """Shorthand: any temperature unit -> Celsius."""
    return convert(value, from_unit, "C")


def to_fahrenheit(value: float, from_unit: str) -> float:
    """Shorthand: any temperature unit -> Fahrenheit."""
    return convert(value, from_unit, "F")


def to_kg(value: float, from_unit: str) -> float:
    """Shorthand: any weight unit -> kilograms."""
    return convert(value, from_unit, "kg")


def to_mg_per_dL_creatinine(value: float, from_unit: str) -> float:
    """Shorthand: creatinine to mg/dL."""
    return convert(value, from_unit, "mg/dL")


def bmi(weight_kg: float, height_m: float) -> float:
    """Compute BMI (kg/m^2)."""
    if height_m <= 0:
        raise ValueError("Height must be > 0 for BMI calculation.")
    return weight_kg / (height_m ** 2)


def bsa_mosteller(weight_kg: float, height_cm: float) -> float:
    """
    Body Surface Area via Mosteller formula (m^2).

    BSA = sqrt( (height_cm * weight_kg) / 3600 )
    """
    if weight_kg <= 0 or height_cm <= 0:
        raise ValueError("Weight and height must be > 0 for BSA calculation.")
    return ((height_cm * weight_kg) / 3600.0) ** 0.5
