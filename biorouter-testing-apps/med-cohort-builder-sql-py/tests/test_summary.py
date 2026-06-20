"""
Tests for summary module.
"""

import os
import tempfile
import sqlite3
import pytest
from med_cohort_builder.summary import CohortSummarizer, CohortSummary
from med_cohort_builder.generate import SyntheticEHRGenerator


@pytest.fixture
def temp_db():
    """Create a temporary database for testing."""
    with tempfile.NamedTemporaryFile(suffix='.db', delete=False) as f:
        db_path = f.name
    
    yield db_path
    
    if os.path.exists(db_path):
        os.remove(db_path)


@pytest.fixture
def populated_db(temp_db):
    """Create a populated database for testing."""
    generator = SyntheticEHRGenerator(seed=42)
    generator.generate_all(temp_db, n_patients=100)
    return temp_db


def test_summarizer_creates_summary(populated_db):
    """Test that summarizer creates a valid summary."""
    summarizer = CohortSummarizer(populated_db)
    
    # Get all patient IDs
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients")
    all_ids = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    summary = summarizer.summarize(all_ids, "Test Cohort")
    
    assert isinstance(summary, CohortSummary)
    assert summary.cohort_name == "Test Cohort"
    assert summary.total_patients == len(all_ids)


def test_summary_age_distribution(populated_db):
    """Test that age distribution is calculated correctly."""
    summarizer = CohortSummarizer(populated_db)
    
    # Get all patient IDs
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients")
    all_ids = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    summary = summarizer.summarize(all_ids, "Test Cohort")
    
    # Check age distribution
    assert len(summary.age_distribution) > 0
    assert sum(summary.age_distribution.values()) == summary.total_patients


def test_summary_sex_distribution(populated_db):
    """Test that sex distribution is calculated correctly."""
    summarizer = CohortSummarizer(populated_db)
    
    # Get all patient IDs
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients")
    all_ids = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    summary = summarizer.summarize(all_ids, "Test Cohort")
    
    # Check sex distribution
    assert 'M' in summary.sex_distribution or 'F' in summary.sex_distribution
    assert sum(summary.sex_distribution.values()) == summary.total_patients


def test_summary_top_diagnoses(populated_db):
    """Test that top diagnoses are identified."""
    summarizer = CohortSummarizer(populated_db)
    
    # Get all patient IDs
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients")
    all_ids = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    summary = summarizer.summarize(all_ids, "Test Cohort")
    
    # Check that we have some diagnoses
    assert len(summary.top_diagnoses) > 0
    
    # Check structure of diagnosis entries
    for diag in summary.top_diagnoses:
        assert 'icd_code' in diag
        assert 'patient_count' in diag
        assert 'total_mentions' in diag
        assert diag['patient_count'] > 0


def test_summary_top_medications(populated_db):
    """Test that top medications are identified."""
    summarizer = CohortSummarizer(populated_db)
    
    # Get all patient IDs
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients")
    all_ids = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    summary = summarizer.summarize(all_ids, "Test Cohort")
    
    # Check that we have some medications
    assert len(summary.top_medications) > 0
    
    # Check structure of medication entries
    for med in summary.top_medications:
        assert 'medication_name' in med
        assert 'patient_count' in med
        assert 'total_prescriptions' in med
        assert med['patient_count'] > 0


def test_summary_encounter_stats(populated_db):
    """Test that encounter statistics are calculated."""
    summarizer = CohortSummarizer(populated_db)
    
    # Get all patient IDs
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients")
    all_ids = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    summary = summarizer.summarize(all_ids, "Test Cohort")
    
    # Check encounter stats
    assert 'total_encounters' in summary.encounter_stats
    assert 'avg_encounters_per_patient' in summary.encounter_stats
    assert summary.encounter_stats['total_encounters'] > 0
    assert summary.encounter_stats['avg_encounters_per_patient'] > 0


def test_summary_mortality_rate(populated_db):
    """Test that mortality rate is calculated."""
    summarizer = CohortSummarizer(populated_db)
    
    # Get all patient IDs
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients")
    all_ids = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    summary = summarizer.summarize(all_ids, "Test Cohort")
    
    # Check mortality rate
    assert 0 <= summary.mortality_rate <= 1


def test_summary_empty_cohort(populated_db):
    """Test summary for empty cohort."""
    summarizer = CohortSummarizer(populated_db)
    
    summary = summarizer.summarize([], "Empty Cohort")
    
    assert summary.total_patients == 0
    assert len(summary.age_distribution) == 0
    assert len(summary.sex_distribution) == 0


def test_summary_serialization(populated_db):
    """Test summary serialization."""
    summarizer = CohortSummarizer(populated_db)
    
    # Get all patient IDs
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients")
    all_ids = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    summary = summarizer.summarize(all_ids, "Test Cohort")
    
    # Convert to dict
    data = summary.to_dict()
    
    assert data['cohort_name'] == "Test Cohort"
    assert data['total_patients'] == len(all_ids)
    assert 'age_distribution' in data
    assert 'sex_distribution' in data
    assert 'top_diagnoses' in data
    assert 'top_medications' in data
    assert 'encounter_stats' in data
    assert 'mortality_rate' in data


def test_summary_subset_cohort(populated_db):
    """Test summary for a subset cohort."""
    summarizer = CohortSummarizer(populated_db)
    
    # Get only male patients
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients WHERE sex = 'M'")
    male_ids = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    summary = summarizer.summarize(male_ids, "Male Patients")
    
    # All should be male
    assert summary.sex_distribution.get('M', 0) == summary.total_patients
    assert summary.total_patients == len(male_ids)


def test_summary_print_summary(populated_db, capsys):
    """Test that summary can be printed."""
    summarizer = CohortSummarizer(populated_db)
    
    # Get all patient IDs
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT patient_id FROM patients")
    all_ids = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    summary = summarizer.summarize(all_ids, "Test Cohort")
    summary.print_summary()
    
    # Check that something was printed
    captured = capsys.readouterr()
    assert "Cohort Summary" in captured.out
    assert "Total Patients" in captured.out
