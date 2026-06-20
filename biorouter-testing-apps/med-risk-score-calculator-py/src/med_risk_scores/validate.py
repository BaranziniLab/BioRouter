"""
Input validation for clinical risk scores.

Validates that supplied inputs meet the declared variable constraints:
types, allowed values, ranges, required-ness, and enum choices.
Produces clear, structured error messages for callers.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Sequence, Tuple, Union


@dataclass(frozen=True)
class VariableSpec:
    """Declaration of a single input variable for a risk score."""

    name: str
    description: str = ""
    var_type: str = "numeric"  # "numeric" | "enum" | "boolean"
    required: bool = True
    min_value: Optional[float] = None
    max_value: Optional[float] = None
    allowed_values: Optional[Sequence[Any]] = None
    unit: Optional[str] = None
    default: Optional[Any] = None


@dataclass
class ValidationError:
    """Structured validation error."""

    variable: str
    message: str
    value: Optional[Any] = None


class ValidationException(Exception):
    """Raised when input validation fails. Carries structured errors."""

    def __init__(self, errors: List[ValidationError]):
        self.errors = errors
        msgs = "; ".join(e.message for e in errors)
        super().__init__(f"Validation failed: {msgs}")


def validate_inputs(
    specs: List[VariableSpec],
    inputs: Dict[str, Any],
    *,
    strict: bool = True,
) -> Dict[str, Any]:
    """
    Validate *inputs* against the given *specs*.

    Returns a dict of validated (and possibly coerced) values on success.
    On failure raises ``ValidationException`` with one ``ValidationError``
    per problem found.

    Parameters
    ----------
    specs : list of VariableSpec
        Variable declarations from the score definition.
    inputs : dict
        User-supplied values keyed by variable name.
    strict : bool
        If True (default), extra keys not in *specs* raise an error.
        If False, unknown keys are silently ignored.
    """
    errors: List[ValidationError] = []
    validated: Dict[str, Any] = {}

    spec_map: Dict[str, VariableSpec] = {s.name: s for s in specs}

    # ---- check for missing / extra keys ----
    provided_keys = set(inputs.keys())
    declared_keys = set(spec_map.keys())

    missing = [k for k in declared_keys if k not in provided_keys and spec_map[k].required and spec_map[k].default is None]
    if missing:
        for m in missing:
            errors.append(ValidationError(variable=m, message=f"Required variable '{m}' is missing."))

    if strict:
        extra = provided_keys - declared_keys
        for e in sorted(extra):
            errors.append(ValidationError(variable=e, message=f"Unexpected variable '{e}'.", value=inputs[e]))

    # ---- validate each provided variable ----
    for spec in specs:
        if spec.name not in inputs:
            # use default if present
            if spec.default is not None:
                validated[spec.name] = spec.default
            continue

        raw = inputs[spec.name]
        coerced = _validate_one(spec, raw, errors)
        if coerced is not _SENTINEL:
            validated[spec.name] = coerced

    if errors:
        raise ValidationException(errors)
    return validated


_SENTINEL = object()


def _validate_one(spec: VariableSpec, raw: Any, errors: List[ValidationError]) -> Any:
    """Validate a single variable; append to *errors* on failure."""
    name = spec.name

    # --- type coercion / checks ---
    if spec.var_type == "boolean":
        coerced = _coerce_bool(raw)
        if coerced is None:
            errors.append(ValidationError(name, f"Cannot interpret '{raw}' as boolean for '{name}'.", raw))
            return _SENTINEL
        return coerced

    if spec.var_type == "enum":
        if spec.allowed_values is None:
            errors.append(ValidationError(name, f"Enum spec for '{name}' has no allowed_values.", raw))
            return _SENTINEL
        if raw not in spec.allowed_values:
            errors.append(
                ValidationError(
                    name,
                    f"Value {raw!r} is not allowed for '{name}'. Must be one of {list(spec.allowed_values)}.",
                    raw,
                )
            )
            return _SENTINEL
        return raw

    # numeric path
    if spec.var_type == "numeric":
        coerced = _coerce_numeric(raw)
        if coerced is None:
            errors.append(ValidationError(name, f"Cannot interpret '{raw}' as a number for '{name}'.", raw))
            return _SENTINEL

        if spec.min_value is not None and coerced < spec.min_value:
            errors.append(
                ValidationError(name, f"{name}={coerced} is below minimum {spec.min_value}.", coerced)
            )
            return _SENTINEL
        if spec.max_value is not None and coerced > spec.max_value:
            errors.append(
                ValidationError(name, f"{name}={coerced} exceeds maximum {spec.max_value}.", coerced)
            )
            return _SENTINEL
        return coerced

    # unknown var_type – pass through
    return raw


# --------------- coercion helpers ---------------

def _coerce_numeric(val: Any) -> Optional[float]:
    if isinstance(val, (int, float)):
        return float(val)
    if isinstance(val, str):
        try:
            return float(val)
        except ValueError:
            return None
    return None


def _coerce_bool(val: Any) -> Optional[bool]:
    if isinstance(val, bool):
        return val
    if isinstance(val, (int, float)):
        return bool(val)
    if isinstance(val, str):
        low = val.strip().lower()
        if low in ("true", "1", "yes", "y"):
            return True
        if low in ("false", "0", "no", "n"):
            return False
    return None
