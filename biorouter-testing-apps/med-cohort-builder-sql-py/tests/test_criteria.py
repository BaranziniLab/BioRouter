"""
Tests for criteria module.
"""

import pytest
from med_cohort_builder.criteria import (
    AgeCriterion, SexCriterion, DiagnosisCriterion,
    MedicationCriterion, LabCriterion, ProcedureCriterion,
    EncounterCriterion, CompoundCriterion, TemporalCriterion,
    CohortDefinition, CriterionType, TemporalRelation, LogicalOperator
)


class TestAgeCriterion:
    """Tests for AgeCriterion."""
    
    def test_min_age_only(self):
        """Test criterion with only min_age."""
        criterion = AgeCriterion(min_age=18)
        sql, params = criterion.to_sql()
        
        assert "julianday('now') - julianday(p.birth_date) >= ? * 365.25" in sql
        assert params == [18]
    
    def test_max_age_only(self):
        """Test criterion with only max_age."""
        criterion = AgeCriterion(max_age=65)
        sql, params = criterion.to_sql()
        
        assert "julianday('now') - julianday(p.birth_date) < ? * 365.25" in sql
        assert params == [66]  # max_age + 1
    
    def test_age_range(self):
        """Test criterion with age range."""
        criterion = AgeCriterion(min_age=18, max_age=65)
        sql, params = criterion.to_sql()
        
        assert ">= ? * 365.25" in sql
        assert "< ? * 365.25" in sql
        assert params == [18, 66]
    
    def test_invalid_age_range(self):
        """Test that invalid age range raises error."""
        with pytest.raises(ValueError, match="min_age cannot be greater than max_age"):
            AgeCriterion(min_age=65, max_age=18)
    
    def test_no_age_specified(self):
        """Test that no age raises error."""
        with pytest.raises(ValueError, match="At least one of min_age or max_age"):
            AgeCriterion()


class TestSexCriterion:
    """Tests for SexCriterion."""
    
    def test_single_sex(self):
        """Test criterion with single sex."""
        criterion = SexCriterion(sex='M')
        sql, params = criterion.to_sql()
        
        assert "p.sex = ?" in sql
        assert params == ['M']
    
    def test_multiple_sexes(self):
        """Test criterion with multiple sexes."""
        criterion = SexCriterion(sex=['M', 'F'])
        sql, params = criterion.to_sql()
        
        assert "p.sex IN (?, ?)" in sql
        assert params == ['M', 'F']


class TestDiagnosisCriterion:
    """Tests for DiagnosisCriterion."""
    
    def test_exact_codes(self):
        """Test criterion with exact ICD codes."""
        criterion = DiagnosisCriterion(icd_codes=['E11.9', 'E11.65'])
        sql, params = criterion.to_sql()
        
        assert "d.icd_code IN (?, ?)" in sql
        assert params == ['E11.9', 'E11.65']
    
    def test_icd_prefix(self):
        """Test criterion with ICD prefix."""
        criterion = DiagnosisCriterion(icd_prefix='E11')
        sql, params = criterion.to_sql()
        
        assert "d.icd_code LIKE ?" in sql
        assert params == ['E11%']
    
    def test_icd_category(self):
        """Test criterion with ICD category."""
        criterion = DiagnosisCriterion(icd_category='diabetes')
        sql, params = criterion.to_sql()
        
        # Should have OR conditions for each diabetes prefix
        assert "OR" in sql
        assert len(params) == 3  # E11, E10, E13
    
    def test_invalid_category(self):
        """Test that invalid category raises error."""
        criterion = DiagnosisCriterion(icd_category='invalid_category')
        with pytest.raises(ValueError, match="Unknown ICD category"):
            criterion.to_sql()
    
    def test_no_criteria_specified(self):
        """Test that no criteria raises error."""
        with pytest.raises(ValueError, match="At least one of"):
            DiagnosisCriterion()


class TestMedicationCriterion:
    """Tests for MedicationCriterion."""
    
    def test_medication_name(self):
        """Test criterion with medication name."""
        criterion = MedicationCriterion(medication_name='Metformin')
        sql, params = criterion.to_sql()
        
        assert "m.medication_name = ?" in sql
        assert params == ['Metformin']
    
    def test_multiple_medications(self):
        """Test criterion with multiple medications."""
        criterion = MedicationCriterion(medication_names=['Aspirin', 'Clopidogrel'])
        sql, params = criterion.to_sql()
        
        assert "m.medication_name IN (?, ?)" in sql
        assert params == ['Aspirin', 'Clopidogrel']
    
    def test_date_range(self):
        """Test criterion with date range."""
        criterion = MedicationCriterion(
            medication_name='Metformin',
            start_date='2020-01-01',
            end_date='2023-12-31'
        )
        sql, params = criterion.to_sql()
        
        assert "m.start_date >= ?" in sql
        assert "m.start_date <= ?" in sql
        assert '2020-01-01' in params
        assert '2023-12-31' in params
    
    def test_within_days(self):
        """Test criterion with within_days."""
        criterion = MedicationCriterion(
            medication_name='Metformin',
            within_days=365
        )
        sql, params = criterion.to_sql()
        
        assert "julianday('now') - julianday(m.start_date) <= ?" in sql
        assert 365 in params


class TestLabCriterion:
    """Tests for LabCriterion."""
    
    def test_lab_name_min_value(self):
        """Test criterion with lab name and min value."""
        criterion = LabCriterion(lab_name='Glucose', min_value=126)
        sql, params = criterion.to_sql()
        
        assert "l.lab_name = ?" in sql
        assert "l.result_value >= ?" in sql
        assert params == ['Glucose', 126]
    
    def test_loinc_code(self):
        """Test criterion with LOINC code."""
        criterion = LabCriterion(loinc_code='4548-4', min_value=6.5)
        sql, params = criterion.to_sql()
        
        assert "l.loinc_code = ?" in sql
        assert params == ['4548-4', 6.5]
    
    def test_abnormal_only(self):
        """Test criterion with abnormal only."""
        criterion = LabCriterion(lab_name='Glucose', abnormal_only=True)
        sql, params = criterion.to_sql()
        
        assert "l.abnormal_flag IN ('H', 'L')" in sql
    
    def test_no_criteria_specified(self):
        """Test that no criteria raises error."""
        with pytest.raises(ValueError, match="At least one of"):
            LabCriterion()


class TestCompoundCriterion:
    """Tests for CompoundCriterion."""
    
    def test_and_operator(self):
        """Test compound criterion with AND."""
        criterion = CompoundCriterion(
            criteria=[AgeCriterion(min_age=18), SexCriterion(sex='M')],
            operator=LogicalOperator.AND
        )
        sql, params = criterion.to_sql()
        
        assert "AND" in sql
        assert 18 in params
        assert 'M' in params
    
    def test_or_operator(self):
        """Test compound criterion with OR."""
        criterion = CompoundCriterion(
            criteria=[
                DiagnosisCriterion(icd_category='diabetes'),
                MedicationCriterion(medication_name='Metformin')
            ],
            operator=LogicalOperator.OR
        )
        sql, params = criterion.to_sql()
        
        assert "OR" in sql
        assert 'Metformin' in params
    
    def test_empty_criteria(self):
        """Test compound criterion with empty criteria."""
        criterion = CompoundCriterion(criteria=[])
        sql, params = criterion.to_sql()
        
        assert sql == "1=1"
        assert params == []


class TestCohortDefinition:
    """Tests for CohortDefinition."""
    
    def test_create_definition(self):
        """Test creating a cohort definition."""
        definition = CohortDefinition(
            name="Test Cohort",
            description="A test cohort"
        )
        
        definition.add_inclusion(AgeCriterion(min_age=18))
        definition.add_exclusion(SexCriterion(sex='O'))
        
        assert definition.name == "Test Cohort"
        assert len(definition.inclusion_criteria) == 1
        assert len(definition.exclusion_criteria) == 1
    
    def test_serialization(self):
        """Test definition serialization."""
        definition = CohortDefinition(
            name="Test Cohort",
            description="A test cohort"
        )
        definition.add_inclusion(AgeCriterion(min_age=18))
        
        # Convert to dict
        data = definition.to_dict()
        
        assert data['name'] == "Test Cohort"
        assert len(data['inclusion_criteria']) == 1
        assert data['inclusion_criteria'][0]['type'] == 'AgeCriterion'
        
        # Convert back
        restored = CohortDefinition.from_dict(data)
        
        assert restored.name == "Test Cohort"
        assert len(restored.inclusion_criteria) == 1


class TestCriterionType:
    """Tests for CriterionType enum."""
    
    def test_inclusion(self):
        """Test inclusion criterion type."""
        criterion = AgeCriterion(min_age=18)
        criterion.include()
        
        assert criterion.criterion_type == CriterionType.INCLUSION
    
    def test_exclusion(self):
        """Test exclusion criterion type."""
        criterion = AgeCriterion(min_age=18)
        criterion.exclude()
        
        assert criterion.criterion_type == CriterionType.EXCLUSION
