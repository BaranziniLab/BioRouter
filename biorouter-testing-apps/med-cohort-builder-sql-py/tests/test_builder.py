"""
Tests for the builder module.
"""

import os
import tempfile
import sqlite3
import pytest
from med_cohort_builder.builder import SQLCompiler, CohortQueryBuilder, SQLQuery
from med_cohort_builder.criteria import (
    AgeCriterion, SexCriterion, DiagnosisCriterion,
    MedicationCriterion, LabCriterion, CohortDefinition
)
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


def test_sql_compiler_creates_valid_query(populated_db):
    """Test that SQL compiler creates valid queries."""
    compiler = SQLCompiler(populated_db)
    
    definition = CohortDefinition(
        name="Test Cohort",
        inclusion_criteria=[AgeCriterion(min_age=18)]
    )
    
    query = compiler.compile(definition)
    
    assert isinstance(query, SQLQuery)
    assert "SELECT DISTINCT p.patient_id" in query.sql
    assert "WHERE" in query.sql
    assert len(query.params) > 0


def test_sql_compiler_execution(populated_db):
    """Test that compiled queries can be executed."""
    compiler = SQLCompiler(populated_db)
    
    definition = CohortDefinition(
        name="Test Cohort",
        inclusion_criteria=[AgeCriterion(min_age=18)]
    )
    
    query = compiler.compile(definition)
    patient_ids = compiler.execute(query)
    
    assert isinstance(patient_ids, list)
    assert len(patient_ids) > 0
    assert all(isinstance(pid, int) for pid in patient_ids)


def test_sql_compiler_cohort_size(populated_db):
    """Test cohort size calculation."""
    compiler = SQLCompiler(populated_db)
    
    definition = CohortDefinition(
        name="Test Cohort",
        inclusion_criteria=[AgeCriterion(min_age=18)]
    )
    
    query = compiler.compile(definition)
    size = compiler.get_cohort_size(query)
    
    assert isinstance(size, int)
    assert size == len(compiler.execute(query))


def test_criteria_filtering_age(populated_db):
    """Test that age criteria filter correctly."""
    compiler = SQLCompiler(populated_db)
    
    # Young patients (18-30)
    definition_young = CohortDefinition(
        name="Young Patients",
        inclusion_criteria=[AgeCriterion(min_age=18, max_age=30)]
    )
    
    query_young = compiler.compile(definition_young)
    young_ids = compiler.execute(query_young)
    
    # Old patients (60+)
    definition_old = CohortDefinition(
        name="Old Patients",
        inclusion_criteria=[AgeCriterion(min_age=60)]
    )
    
    query_old = compiler.compile(definition_old)
    old_ids = compiler.execute(query_old)
    
    # Young and old should be disjoint
    assert len(set(young_ids) & set(old_ids)) == 0
    
    # Both should be subsets of all adult patients
    definition_all = CohortDefinition(
        name="All Adults",
        inclusion_criteria=[AgeCriterion(min_age=18)]
    )
    
    query_all = compiler.compile(definition_all)
    all_adult_ids = compiler.execute(query_all)
    
    assert set(young_ids).issubset(set(all_adult_ids))
    assert set(old_ids).issubset(set(all_adult_ids))


def test_criteria_filtering_sex(populated_db):
    """Test that sex criteria filter correctly."""
    compiler = SQLCompiler(populated_db)
    
    # Male patients
    definition_male = CohortDefinition(
        name="Male Patients",
        inclusion_criteria=[SexCriterion(sex='M')]
    )
    
    query_male = compiler.compile(definition_male)
    male_ids = compiler.execute(query_male)
    
    # Female patients
    definition_female = CohortDefinition(
        name="Female Patients",
        inclusion_criteria=[SexCriterion(sex='F')]
    )
    
    query_female = compiler.compile(definition_female)
    female_ids = compiler.execute(query_female)
    
    # Male and female should be disjoint
    assert len(set(male_ids) & set(female_ids)) == 0
    
    # Total should equal all patients
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    cursor.execute("SELECT COUNT(*) FROM patients")
    total = cursor.fetchone()[0]
    conn.close()
    
    assert len(male_ids) + len(female_ids) <= total


def test_criteria_filtering_diagnosis(populated_db):
    """Test that diagnosis criteria filter correctly."""
    compiler = SQLCompiler(populated_db)
    
    # Patients with diabetes
    definition_diabetes = CohortDefinition(
        name="Diabetic Patients",
        inclusion_criteria=[DiagnosisCriterion(icd_prefix='E11')]
    )
    
    query_diabetes = compiler.compile(definition_diabetes)
    diabetes_ids = compiler.execute(query_diabetes)
    
    # Verify all returned patients have diabetes diagnosis
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    
    placeholders = ", ".join(["?" for _ in diabetes_ids])
    cursor.execute(f"""
        SELECT DISTINCT patient_id 
        FROM diagnoses 
        WHERE patient_id IN ({placeholders})
        AND icd_code LIKE 'E11%'
    """, diabetes_ids)
    
    verified_ids = set(row[0] for row in cursor.fetchall())
    conn.close()
    
    assert set(diabetes_ids) == verified_ids


def test_compound_and_criteria(populated_db):
    """Test compound AND criteria."""
    builder = CohortQueryBuilder(populated_db)
    
    definition = CohortDefinition(
        name="Young Males",
        inclusion_criteria=[
            AgeCriterion(min_age=18, max_age=30),
            SexCriterion(sex='M')
        ]
    )
    
    builder.definition = definition
    patient_ids = builder.execute()
    
    # Verify all returned patients are young males
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    
    placeholders = ", ".join(["?" for _ in patient_ids])
    cursor.execute(f"""
        SELECT patient_id, 
               (julianday('now') - julianday(birth_date)) / 365.25 as age,
               sex
        FROM patients 
        WHERE patient_id IN ({placeholders})
    """, patient_ids)
    
    for row in cursor.fetchall():
        pid, age, sex = row
        assert 18 <= age < 31, f"Patient {pid} has age {age}"
        assert sex == 'M', f"Patient {pid} has sex {sex}"
    
    conn.close()


def test_compound_or_criteria(populated_db):
    """Test compound OR criteria."""
    from med_cohort_builder.criteria import CompoundCriterion, LogicalOperator
    
    builder = CohortQueryBuilder(populated_db)
    
    # Patients with diabetes OR hypertension
    definition = CohortDefinition(
        name="Diabetes or Hypertension",
        inclusion_criteria=[
            CompoundCriterion(
                criteria=[
                    DiagnosisCriterion(icd_prefix='E11'),
                    DiagnosisCriterion(icd_prefix='I10')
                ],
                operator=LogicalOperator.OR
            )
        ]
    )
    
    builder.definition = definition
    patient_ids = builder.execute()
    
    # Verify all returned patients have at least one condition
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    
    placeholders = ", ".join(["?" for _ in patient_ids])
    cursor.execute(f"""
        SELECT DISTINCT patient_id 
        FROM diagnoses 
        WHERE patient_id IN ({placeholders})
        AND (icd_code LIKE 'E11%' OR icd_code LIKE 'I10%')
    """, patient_ids)
    
    verified_ids = set(row[0] for row in cursor.fetchall())
    conn.close()
    
    assert set(patient_ids) == verified_ids


def test_exclusion_criteria(populated_db):
    """Test exclusion criteria."""
    builder = CohortQueryBuilder(populated_db)
    
    # All adults excluding females
    definition = CohortDefinition(
        name="Non-Female Adults",
        inclusion_criteria=[AgeCriterion(min_age=18)],
        exclusion_criteria=[SexCriterion(sex='F')]
    )
    
    builder.definition = definition
    patient_ids = builder.execute()
    
    # Verify all returned patients are NOT female (male or other)
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    
    placeholders = ", ".join(["?" for _ in patient_ids])
    cursor.execute(f"""
        SELECT sex FROM patients 
        WHERE patient_id IN ({placeholders})
    """, patient_ids)
    
    sexes = set(row[0] for row in cursor.fetchall())
    conn.close()
    
    # Should only have M and/or O, no F
    assert 'F' not in sexes
    assert len(patient_ids) > 0


def test_fluent_builder_api(populated_db):
    """Test fluent builder API."""
    builder = CohortQueryBuilder(populated_db)
    
    patient_ids = (
        builder
        .set_name("Fluent Cohort")
        .set_description("Testing fluent API")
        .include(AgeCriterion(min_age=18))
        .include(SexCriterion(sex='M'))
        .execute()
    )
    
    assert len(patient_ids) > 0
    
    # Verify all are adult males
    conn = sqlite3.connect(populated_db)
    cursor = conn.cursor()
    
    placeholders = ", ".join(["?" for _ in patient_ids])
    cursor.execute(f"""
        SELECT sex, 
               (julianday('now') - julianday(birth_date)) / 365.25 as age
        FROM patients 
        WHERE patient_id IN ({placeholders})
    """, patient_ids)
    
    for row in cursor.fetchall():
        sex, age = row
        assert sex == 'M'
        assert age >= 18
    
    conn.close()


def test_empty_cohort(populated_db):
    """Test that impossible criteria return empty cohort."""
    builder = CohortQueryBuilder(populated_db)
    
    # Impossible criteria: age 5-10 (adults only in our data)
    definition = CohortDefinition(
        name="Impossible Cohort",
        inclusion_criteria=[AgeCriterion(min_age=5, max_age=10)]
    )
    
    builder.definition = definition
    patient_ids = builder.execute()
    
    assert len(patient_ids) == 0
