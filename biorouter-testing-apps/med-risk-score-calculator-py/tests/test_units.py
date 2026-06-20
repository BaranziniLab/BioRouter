"""Tests for unit conversion helpers."""
import math
import pytest
from med_risk_scores.units import (
    convert,
    to_celsius,
    to_fahrenheit,
    to_kg,
    to_mg_per_dL_creatinine,
    bmi,
    bsa_mosteller,
)


class TestConvertTemperature:
    def test_f_to_c(self):
        assert convert(98.6, "F", "C") == pytest.approx(37.0, abs=0.05)

    def test_c_to_f(self):
        assert convert(37.0, "C", "F") == pytest.approx(98.6, abs=0.05)

    def test_c_to_c(self):
        assert convert(37.0, "C", "C") == 37.0

    def test_f_to_f(self):
        assert convert(98.6, "F", "F") == 98.6

    def test_boiling_point_c_to_f(self):
        assert convert(100, "C", "F") == pytest.approx(212.0, abs=0.1)

    def test_freezing_point_c_to_f(self):
        assert convert(0, "C", "F") == pytest.approx(32.0, abs=0.1)

    def test_to_celsius_shorthand(self):
        assert to_celsius(98.6, "F") == pytest.approx(37.0, abs=0.05)

    def test_to_fahrenheit_shorthand(self):
        assert to_fahrenheit(37.0, "C") == pytest.approx(98.6, abs=0.05)


class TestConvertPressure:
    def test_mmhg_to_kpa(self):
        assert convert(760, "mmHg", "kPa") == pytest.approx(101.325, abs=0.5)

    def test_kpa_to_mmhg(self):
        assert convert(101.325, "kPa", "mmHg") == pytest.approx(760, abs=1)

    def test_blood_pressure(self):
        # 120 mmHg -> kPa
        kpa = convert(120, "mmHg", "kPa")
        assert 15 < kpa < 17


class TestConvertWeight:
    def test_kg_to_lb(self):
        assert convert(70, "kg", "lb") == pytest.approx(154.32, abs=0.5)

    def test_lb_to_kg(self):
        assert convert(154, "lb", "kg") == pytest.approx(69.85, abs=0.5)

    def test_kg_to_g(self):
        assert convert(1.5, "kg", "g") == 1500.0

    def test_to_kg_shorthand(self):
        assert to_kg(154, "lb") == pytest.approx(69.85, abs=0.5)


class TestConvertHeight:
    def test_cm_to_in(self):
        assert convert(180, "cm", "in") == pytest.approx(70.87, abs=0.1)

    def test_in_to_cm(self):
        assert convert(70, "in", "cm") == pytest.approx(177.8, abs=0.1)

    def test_cm_to_m(self):
        assert convert(175, "cm", "m") == pytest.approx(1.75, abs=0.01)


class TestConvertVolume:
    def test_dL_to_L(self):
        assert convert(5, "dL", "L") == pytest.approx(0.5, abs=0.01)

    def test_L_to_mL(self):
        assert convert(1.5, "L", "mL") == 1500.0

    def test_mL_to_dL(self):
        assert convert(250, "mL", "dL") == pytest.approx(2.5, abs=0.01)


class TestConvertCreatinine:
    def test_mg_dl_to_umol(self):
        assert convert(1.0, "mg/dL", "µmol/L") == pytest.approx(88.4, abs=0.1)

    def test_umol_to_mg_dl(self):
        assert to_mg_per_dL_creatinine(88.4, "µmol/L") == pytest.approx(1.0, abs=0.01)


class TestConvertErrors:
    def test_unknown_pair(self):
        with pytest.raises(ValueError, match="Unknown conversion"):
            convert(100, "kg", "mmHg")


class TestBMI:
    def test_normal(self):
        # 70 kg, 1.75 m -> 22.86
        assert bmi(70, 1.75) == pytest.approx(22.857, abs=0.01)

    def test_obese(self):
        assert bmi(120, 1.70) == pytest.approx(41.52, abs=0.1)

    def test_underweight(self):
        assert bmi(45, 1.70) == pytest.approx(15.57, abs=0.1)

    def test_zero_height_raises(self):
        with pytest.raises(ValueError, match="Height must be > 0"):
            bmi(70, 0)


class TestBSA:
    def test_average_male(self):
        # 70 kg, 175 cm -> sqrt(70*175/3600) = sqrt(3.4028) ≈ 1.845 m^2
        assert bsa_mosteller(70, 175) == pytest.approx(1.845, abs=0.01)

    def test_zero_weight_raises(self):
        with pytest.raises(ValueError):
            bsa_mosteller(0, 170)

    def test_zero_height_raises(self):
        with pytest.raises(ValueError):
            bsa_mosteller(70, 0)
