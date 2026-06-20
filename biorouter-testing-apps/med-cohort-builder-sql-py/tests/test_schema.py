"""
Tests for schema module.
"""

import os
import tempfile
import pytest
from med_cohort_builder.schema import create_database, get_schema_info, drop_database


@pytest.fixture
def temp_db():
    """Create a temporary database for testing."""
    with tempfile.NamedTemporaryFile(suffix='.db', delete=False) as f:
        db_path = f.name
    
    yield db_path
    
    # Cleanup
    if os.path.exists(db_path):
        os.remove(db_path)


def test_create_database(temp_db):
    """Test database creation."""
    create_database(temp_db)
    
    assert os.path.exists(temp_db)
    
    schema = get_schema_info(temp_db)
    
    # Check that all tables exist
    expected_tables = ['patients', 'encounters', 'diagnoses', 'medications', 'labs', 'procedures']
    for table in expected_tables:
        assert table in schema, f"Table {table} not found in schema"


def test_schema_columns(temp_db):
    """Test that tables have expected columns."""
    create_database(temp_db)
    
    schema = get_schema_info(temp_db)
    
    # Check patients table columns
    assert 'patient_id' in schema['patients']
    assert 'birth_date' in schema['patients']
    assert 'sex' in schema['patients']
    
    # Check encounters table columns
    assert 'encounter_id' in schema['encounters']
    assert 'patient_id' in schema['encounters']
    assert 'encounter_date' in schema['encounters']
    
    # Check diagnoses table columns
    assert 'diagnosis_id' in schema['diagnoses']
    assert 'icd_code' in schema['diagnoses']
    assert 'icd_version' in schema['diagnoses']


def test_drop_database(temp_db):
    """Test database dropping."""
    create_database(temp_db)
    
    # Verify it exists
    assert os.path.exists(temp_db)
    
    # Drop it
    drop_database(temp_db)
    
    # Verify tables are gone
    schema = get_schema_info(temp_db)
    assert len(schema) == 0


def test_create_database_idempotent(temp_db):
    """Test that creating database twice doesn't fail."""
    create_database(temp_db)
    create_database(temp_db)  # Should not raise
    
    schema = get_schema_info(temp_db)
    assert 'patients' in schema
