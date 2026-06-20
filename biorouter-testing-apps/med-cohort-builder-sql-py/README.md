# Med Cohort Builder

A cohort-builder over synthetic EHR (Electronic Health Records) using SQLite in Python.

## Overview

This project provides tools for building patient cohorts from synthetic EHR data. It includes:

- **Synthetic Data Generator**: Creates realistic synthetic EHR data including patients, encounters, diagnoses, medications, labs, and procedures
- **Cohort Query Builder**: Fluent/declarative API to define inclusion/exclusion criteria
- **SQL Compiler**: Converts criteria to parameterized SQL queries
- **Summary Statistics**: Calculate cohort demographics, top diagnoses, medications, etc.
- **Prevalence Calculator**: Calculate point prevalence, period prevalence, and incidence rates
- **CLI Interface**: Command-line tools for data generation and cohort building

## Project Structure

```
med-cohort-builder-sql-py/
├── src/
│   └── med_cohort_builder/
│       ├── __init__.py          # Package initialization
│       ├── schema.py            # Database schema definitions
│       ├── generate.py          # Synthetic data generator
│       ├── criteria.py          # Cohort criteria definitions
│       ├── builder.py           # SQL compiler for criteria
│       ├── summary.py           # Cohort summary statistics
│       ├── prevalence.py        # Incidence/prevalence calculator
│       └── cli.py               # Command-line interface
├── tests/
│   ├── test_schema.py           # Schema tests
│   ├── test_generate.py         # Generator tests
│   ├── test_criteria.py         # Criteria tests
│   ├── test_builder.py          # Builder tests
│   └── test_summary.py          # Summary tests
├── pyproject.toml               # Project configuration
└── README.md                    # This file
```

## Installation

```bash
# Clone the repository
git clone <repository-url>
cd med-cohort-builder-sql-py

# Install in development mode
pip install -e .

# Install test dependencies
pip install pytest
```

## Quick Start

### 1. Generate Synthetic Data

```bash
# Generate a database with 100 patients
python -m med_cohort_builder generate my_ehr.db --patients 100

# Generate with reproducible results
python -m med_cohort_builder generate my_ehr.db --patients 100 --seed 42
```

### 2. Build a Cohort

Using the Python API:

```python
from med_cohort_builder import (
    CohortQueryBuilder, 
    AgeCriterion, 
    SexCriterion, 
    DiagnosisCriterion
)

# Build a cohort of adult males with diabetes
builder = CohortQueryBuilder("my_ehr.db")
patient_ids = (
    builder
    .set_name("Diabetic Males")
    .include(AgeCriterion(min_age=18))
    .include(SexCriterion(sex='M'))
    .include(DiagnosisCriterion(icd_prefix='E11'))
    .execute()
)

print(f"Found {len(patient_ids)} patients")
```

Or using a JSON definition file:

```json
{
  "name": "Diabetic Patients",
  "description": "Adult patients with Type 2 diabetes",
  "inclusion_criteria": [
    {"type": "AgeCriterion", "min_age": 18},
    {"type": "DiagnosisCriterion", "icd_prefix": "E11"}
  ],
  "exclusion_criteria": [
    {"type": "SexCriterion", "sex": "O"}
  ]
}
```

```bash
python -m med_cohort_builder build my_ehr.db cohort_def.json -o results.csv
```

### 3. Get Cohort Summary

```python
from med_cohort_builder import CohortSummarizer

summarizer = CohortSummarizer("my_ehr.db")
summary = summarizer.summarize(patient_ids, "Diabetic Males")
summary.print_summary()
```

### 4. Calculate Prevalence

```python
from med_cohort_builder import PrevalenceCalculator

calculator = PrevalenceCalculator("my_ehr.db")

# Point prevalence of diabetes on 2023-01-01
result = calculator.calculate_diagnosis_prevalence(
    patient_ids,
    icd_prefix='E11',
    prevalence_date='2023-01-01'
)

print(f"Prevalence: {result.percentage:.2f}%")
```

## Criteria Types

### Age Criterion
```python
AgeCriterion(min_age=18)           # 18 or older
AgeCriterion(max_age=65)           # Under 66
AgeCriterion(min_age=18, max_age=65)  # 18-65
```

### Sex Criterion
```python
SexCriterion(sex='M')              # Male
SexCriterion(sex=['M', 'F'])       # Male or Female
```

### Diagnosis Criterion
```python
DiagnosisCriterion(icd_codes=['E11.9', 'E11.65'])  # Exact codes
DiagnosisCriterion(icd_prefix='E11')                 # All E11.* codes
DiagnosisCriterion(icd_category='diabetes')           # Predefined category
```

### Medication Criterion
```python
MedicationCriterion(medication_name='Metformin')
MedicationCriterion(medication_names=['Aspirin', 'Clopidogrel'])
MedicationCriterion(ndc_code='00093105601')
```

### Lab Criterion
```python
LabCriterion(lab_name='Glucose', min_value=126)
LabCriterion(loinc_code='4548-4', min_value=6.5)  # HbA1c
LabCriterion(lab_name='Glucose', abnormal_only=True)
```

### Procedure Criterion
```python
ProcedureCriterion(procedure_code='99213')
ProcedureCriterion(procedure_name='Chest X-ray')
```

### Compound Criteria
```python
from med_cohort_builder import CompoundCriterion, LogicalOperator

# AND logic
CompoundCriterion(
    criteria=[AgeCriterion(min_age=18), SexCriterion(sex='M')],
    operator=LogicalOperator.AND
)

# OR logic
CompoundCriterion(
    criteria=[
        DiagnosisCriterion(icd_prefix='E11'),
        MedicationCriterion(medication_name='Metformin')
    ],
    operator=LogicalOperator.OR
)
```

## Running Tests

```bash
# Run all tests
pytest

# Run with verbose output
pytest -v

# Run specific test file
pytest tests/test_criteria.py

# Run tests with coverage
pytest --cov=med_cohort_builder
```

## Synthetic Data Schema

The generator creates the following tables:

- **patients**: Patient demographics (ID, birth date, death date, sex, race, ethnicity)
- **encounters**: Healthcare encounters (type, department, facility)
- **diagnoses**: ICD-9/10 diagnosis codes
- **medications**: Medication prescriptions (NDC codes, dates, dosages)
- **labs**: Laboratory test results (LOINC codes, values, units)
- **procedures**: Medical procedures (CPT codes)

## License

MIT License
