"""
Tests for the generate module.
"""

import os
import tempfile
import sqlite3
import pytest
from med_cohort_builder.generate import SyntheticEHRGenerator
from med_cohort_builder.schema import create_database


@pytest.fixture
def temp_db():
    """Create a temporary database for testing."""
    with tempfile.NamedTemporaryFile(suffix='.db', delete=False) as f:
        db_path = f.name
    
    yield db_path
    
    if os.path.exists(db_path):
        os.remove(db_path)


@pytest.fixture
def seeded_generator():
    """Create a seeded generator for reproducible tests."""
    return SyntheticEHRGenerator(seed=42)


def test_generator_creates_valid_database(seeded_generator, temp_db):
    """Test that generator creates a valid database."""
    seeded_generator.generate_all(temp_db, n_patients=50)
    
    assert os.path.exists(temp_db)
    
    conn = sqlite3.connect(temp_db)
    cursor = conn.cursor()
    
    # Check that tables have data
    cursor.execute("SELECT COUNT(*) FROM patients")
    patient_count = cursor.fetchone()[0]
    assert patient_count == 50
    
    cursor.execute("SELECT COUNT(*) FROM encounters")
    encounter_count = cursor.fetchone()[0]
    assert encounter_count > 0
    
    cursor.execute("SELECT COUNT(*) FROM diagnoses")
    diagnosis_count = cursor.fetchone()[0]
    assert diagnosis_count > 0
    
    conn.close()


def test_generator_patient_attributes(seeded_generator, temp_db):
    """Test that generated patients have valid attributes."""
    seeded_generator.generate_all(temp_db, n_patients=100)
    
    conn = sqlite3.connect(temp_db)
    cursor = conn.cursor()
    
    cursor.execute("SELECT sex FROM patients")
    sexes = [row[0] for row in cursor.fetchall()]
    
    # Check sex values are valid
    valid_sexes = {'M', 'F', 'O'}
    for sex in sexes:
        assert sex in valid_sexes, f"Invalid sex value: {sex}"
    
    # Check distribution is reasonable (not all same)
    from collections import Counter
    sex_counts = Counter(sexes)
    assert len(sex_counts) >= 2, "Expected at least 2 different sex values"
    
    conn.close()


def test_generator_diagnosis_codes(seeded_generator, temp_db):
    """Test that generated diagnoses have valid ICD codes."""
    seeded_generator.generate_all(temp_db, n_patients=50)
    
    conn = sqlite3.connect(temp_db)
    cursor = conn.cursor()
    
    cursor.execute("SELECT DISTINCT icd_code FROM diagnoses LIMIT 10")
    codes = [row[0] for row in cursor.fetchall()]
    
    # Check that codes are non-empty strings
    for code in codes:
        assert isinstance(code, str)
        assert len(code) > 0
        assert len(code) <= 10  # ICD codes shouldn't be too long
    
    conn.close()


def test_generator_reproducibility(temp_db):
    """Test that seeded generator produces same results."""
    gen1 = SyntheticEHRGenerator(seed=123)
    gen1.generate_all(temp_db, n_patients=25)
    
    # Get first set of patient IDs
    conn = sqlite3.connect(temp_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients ORDER BY patient_id")
    ids1 = cursor.fetchall()
    conn.close()
    
    # Delete and regenerate
    os.remove(temp_db)
    
    gen2 = SyntheticEHRGenerator(seed=123)
    gen2.generate_all(temp_db, n_patients=25)
    
    conn = sqlite3.connect(temp_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients ORDER BY patient_id")
    ids2 = cursor.fetchall()
    conn.close()
    
    # Should be identical (same patient IDs generated in same order)
    assert ids1 == ids2


def test_generator_different_seeds():
    """Test that different seeds produce different results."""
    with tempfile.NamedTemporaryFile(suffix='.db', delete=False) as f1:
        db1 = f1.name
    with tempfile.NamedTemporaryFile(suffix='.db', delete=False) as f2:
        db2 = f2.name
    
    try:
        gen1 = SyntheticEHRGenerator(seed=111)
        gen1.generate_all(db1, n_patients=50)
        
        gen2 = SyntheticEHRGenerator(seed=222)
        gen2.generate_all(db2, n_patients=50)
        
        conn1 = sqlite3.connect(db1)
        conn2 = sqlite3.connect(db2)
        
        cursor1 = conn1.cursor()
        cursor2 = conn2.cursor()
        
        # Check that zip codes differ (statistically should be different)
        cursor1.execute("SELECT address_zip FROM patients LIMIT 10")
        cursor2.execute("SELECT address_zip FROM patients LIMIT 10")
        
        zips1 = set(row[0] for row in cursor1.fetchall())
        zips2 = set(row[0] for row in cursor2.fetchall())
        
        # At least some zips should be different
        assert zips1 != zips2, "Different seeds should produce different data"
        
        conn1.close()
        conn2.close()
        
    finally:
        if os.path.exists(db1):
            os.remove(db1)
        if os.path.exists(db2):
            os.remove(db2)


def test_generator_medication_structure(seeded_generator, temp_db):
    """Test that medications have proper structure."""
    seeded_generator.generate_all(temp_db, n_patients=50)
    
    conn = sqlite3.connect(temp_db)
    cursor = conn.cursor()
    
    cursor.execute("""
        SELECT medication_name, ndc_code, start_date, dosage, route
        FROM medications
        LIMIT 20
    """)
    
    for row in cursor.fetchall():
        name, ndc, start_date, dosage, route = row
        
        assert name is not None and len(name) > 0
        assert start_date is not None
        assert route in ['oral', 'injection', 'topical']
        
    conn.close()


def test_generator_lab_values(seeded_generator, temp_db):
    """Test that lab values are reasonable."""
    seeded_generator.generate_all(temp_db, n_patients=50)
    
    conn = sqlite3.connect(temp_db)
    cursor = conn.cursor()
    
    cursor.execute("""
        SELECT lab_name, result_value, result_unit, abnormal_flag
        FROM labs
        WHERE lab_name = 'Glucose'
        LIMIT 20
    """)
    
    for row in cursor.fetchall():
        name, value, unit, flag = row
        
        # Glucose should be positive and in reasonable range
        assert value > 0, f"Glucose value should be positive: {value}"
        assert value < 1000, f"Glucose value seems too high: {value}"
        assert flag in ['H', 'L', 'N', None]
        
    conn.close()
