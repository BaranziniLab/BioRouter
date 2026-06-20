"""
Med Cohort Builder - A cohort-builder over synthetic EHR using SQLite.
"""

__version__ = "0.1.0"
__author__ = "Med Cohort Builder Team"

from .schema import create_database, get_schema_info, drop_database
from .generate import SyntheticEHRGenerator
from .criteria import (
    AgeCriterion, SexCriterion, DiagnosisCriterion, 
    MedicationCriterion, LabCriterion, ProcedureCriterion,
    EncounterCriterion, CompoundCriterion, TemporalCriterion,
    CohortDefinition, CriterionType, TemporalRelation, LogicalOperator
)
from .builder import SQLCompiler, CohortQueryBuilder, SQLQuery
from .summary import CohortSummarizer, CohortSummary
from .prevalence import PrevalenceCalculator, PrevalenceResult, PrevalenceType

__all__ = [
    # Schema
    "create_database",
    "get_schema_info", 
    "drop_database",
    
    # Generator
    "SyntheticEHRGenerator",
    
    # Criteria
    "AgeCriterion",
    "SexCriterion",
    "DiagnosisCriterion",
    "MedicationCriterion",
    "LabCriterion",
    "ProcedureCriterion",
    "EncounterCriterion",
    "CompoundCriterion",
    "TemporalCriterion",
    "CohortDefinition",
    "CriterionType",
    "TemporalRelation",
    "LogicalOperator",
    
    # Builder
    "SQLCompiler",
    "CohortQueryBuilder",
    "SQLQuery",
    
    # Summary
    "CohortSummarizer",
    "CohortSummary",
    
    # Prevalence
    "PrevalenceCalculator",
    "PrevalenceResult",
    "PrevalenceType",
]
