"""Tests for input validation module."""
import pytest
from med_risk_scores.validate import (
    VariableSpec,
    validate_inputs,
    ValidationException,
    ValidationError,
)


class TestVariableSpec:
    def test_basic_numeric_spec(self):
        s = VariableSpec(name="age", var_type="numeric", min_value=0, max_value=130)
        assert s.name == "age"
        assert s.required is True
        assert s.min_value == 0

    def test_enum_spec(self):
        s = VariableSpec(name="sex", var_type="enum", allowed_values=["male", "female"])
        assert s.allowed_values == ["male", "female"]

    def test_default_value(self):
        s = VariableSpec(name="flag", var_type="boolean", required=False, default=False)
        assert s.default is False


class TestValidateInputs:
    def test_valid_numeric(self):
        specs = [VariableSpec(name="age", var_type="numeric", min_value=0, max_value=130)]
        result = validate_inputs(specs, {"age": 72})
        assert result["age"] == 72.0

    def test_valid_numeric_string_coercion(self):
        specs = [VariableSpec(name="age", var_type="numeric", min_value=0, max_value=130)]
        result = validate_inputs(specs, {"age": "45"})
        assert result["age"] == 45.0

    def test_valid_boolean(self):
        specs = [VariableSpec(name="smoker", var_type="boolean")]
        assert validate_inputs(specs, {"smoker": True}) == {"smoker": True}
        assert validate_inputs(specs, {"smoker": "yes"}) == {"smoker": True}
        assert validate_inputs(specs, {"smoker": "no"}) == {"smoker": False}
        assert validate_inputs(specs, {"smoker": 0}) == {"smoker": False}
        assert validate_inputs(specs, {"smoker": 1}) == {"smoker": True}

    def test_valid_enum(self):
        specs = [VariableSpec(name="sex", var_type="enum", allowed_values=["M", "F"])]
        result = validate_inputs(specs, {"sex": "M"})
        assert result["sex"] == "M"

    def test_missing_required(self):
        specs = [VariableSpec(name="age", var_type="numeric", required=True)]
        with pytest.raises(ValidationException) as exc_info:
            validate_inputs(specs, {})
        assert len(exc_info.value.errors) == 1
        assert "missing" in exc_info.value.errors[0].message.lower()

    def test_missing_optional_with_default(self):
        specs = [VariableSpec(name="flag", var_type="boolean", required=False, default=False)]
        result = validate_inputs(specs, {})
        assert result["flag"] is False

    def test_extra_key_rejected_in_strict(self):
        specs = [VariableSpec(name="age", var_type="numeric")]
        with pytest.raises(ValidationException) as exc_info:
            validate_inputs(specs, {"age": 50, "bogus": 1})
        msgs = [e.message for e in exc_info.value.errors]
        assert any("bogus" in m for m in msgs)

    def test_extra_key_ignored_in_non_strict(self):
        specs = [VariableSpec(name="age", var_type="numeric")]
        result = validate_inputs(specs, {"age": 50, "bogus": 1}, strict=False)
        assert result["age"] == 50.0
        assert "bogus" not in result

    def test_below_min_value(self):
        specs = [VariableSpec(name="age", var_type="numeric", min_value=0, max_value=130)]
        with pytest.raises(ValidationException) as exc_info:
            validate_inputs(specs, {"age": -5})
        assert any("below minimum" in e.message for e in exc_info.value.errors)

    def test_above_max_value(self):
        specs = [VariableSpec(name="age", var_type="numeric", min_value=0, max_value=130)]
        with pytest.raises(ValidationException) as exc_info:
            validate_inputs(specs, {"age": 200})
        assert any("exceeds maximum" in e.message for e in exc_info.value.errors)

    def test_invalid_enum_value(self):
        specs = [VariableSpec(name="sex", var_type="enum", allowed_values=["M", "F"])]
        with pytest.raises(ValidationException) as exc_info:
            validate_inputs(specs, {"sex": "X"})
        assert any("not allowed" in e.message for e in exc_info.value.errors)

    def test_non_numeric_string(self):
        specs = [VariableSpec(name="age", var_type="numeric")]
        with pytest.raises(ValidationException) as exc_info:
            validate_inputs(specs, {"age": "abc"})
        assert any("Cannot interpret" in e.message for e in exc_info.value.errors)

    def test_invalid_boolean(self):
        specs = [VariableSpec(name="flag", var_type="boolean")]
        with pytest.raises(ValidationException) as exc_info:
            validate_inputs(specs, {"flag": "maybe"})
        assert any("boolean" in e.message.lower() for e in exc_info.value.errors)

    def test_multiple_errors_collected(self):
        specs = [
            VariableSpec(name="age", var_type="numeric", min_value=0, max_value=130, required=True),
            VariableSpec(name="sex", var_type="enum", allowed_values=["M", "F"], required=True),
        ]
        with pytest.raises(ValidationException) as exc_info:
            validate_inputs(specs, {"sex": "X"})
        # Missing 'age' and invalid 'sex'
        assert len(exc_info.value.errors) == 2

    def test_boundary_min(self):
        specs = [VariableSpec(name="val", var_type="numeric", min_value=0, max_value=100)]
        result = validate_inputs(specs, {"val": 0})
        assert result["val"] == 0.0

    def test_boundary_max(self):
        specs = [VariableSpec(name="val", var_type="numeric", min_value=0, max_value=100)]
        result = validate_inputs(specs, {"val": 100})
        assert result["val"] == 100.0

    def test_validation_exception_str(self):
        exc = ValidationException([
            ValidationError("age", "age is missing"),
            ValidationError("sex", "sex is invalid"),
        ])
        assert "age is missing" in str(exc)
        assert "sex is invalid" in str(exc)
