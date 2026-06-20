"""
Cohort criteria definitions.
Fluent/declarative API to define inclusion/exclusion criteria for patient cohorts.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import List, Optional, Union, Any
from enum import Enum
from datetime import datetime, timedelta


class CriterionType(Enum):
    """Types of criteria."""
    INCLUSION = "inclusion"
    EXCLUSION = "exclusion"


class TemporalRelation(Enum):
    """Temporal relationships between events."""
    BEFORE = "before"
    AFTER = "after"
    WITHIN_DAYS = "within_days"
    ON_SAME_DAY = "on_same_day"
    OVERLAPPING = "overlapping"


class LogicalOperator(Enum):
    """Logical operators for combining criteria."""
    AND = "AND"
    OR = "OR"


@dataclass
class Criterion(ABC):
    """
    Base class for all cohort criteria.
    """
    criterion_type: CriterionType = CriterionType.INCLUSION
    description: str = ""
    
    @abstractmethod
    def to_sql(self) -> tuple:
        """
        Convert criterion to SQL WHERE clause.
        
        Returns:
            Tuple of (sql_clause, parameters)
        """
        pass
    
    def include(self) -> 'Criterion':
        """Mark as inclusion criterion."""
        self.criterion_type = CriterionType.INCLUSION
        return self
    
    def exclude(self) -> 'Criterion':
        """Mark as exclusion criterion."""
        self.criterion_type = CriterionType.EXCLUSION
        return self


@dataclass
class AgeCriterion(Criterion):
    """
    Filter patients by age.
    
    Examples:
        AgeCriterion(min_age=18, max_age=65)
        AgeCriterion(min_age=50)  # 50 years or older
    """
    min_age: Optional[int] = None
    max_age: Optional[int] = None
    
    def __post_init__(self):
        if self.min_age is None and self.max_age is None:
            raise ValueError("At least one of min_age or max_age must be specified")
        if self.min_age is not None and self.max_age is not None:
            if self.min_age > self.max_age:
                raise ValueError("min_age cannot be greater than max_age")
    
    def to_sql(self) -> tuple:
        conditions = []
        params = []
        
        if self.min_age is not None:
            conditions.append("julianday('now') - julianday(p.birth_date) >= ? * 365.25")
            params.append(self.min_age)
        
        if self.max_age is not None:
            conditions.append("julianday('now') - julianday(p.birth_date) < ? * 365.25")
            params.append(self.max_age + 1)
        
        return (" AND ".join(conditions), params)


@dataclass
class SexCriterion(Criterion):
    """
    Filter patients by biological sex.
    
    Examples:
        SexCriterion(sex='M')
        SexCriterion(sex=['M', 'F'])
    """
    sex: Union[str, List[str]] = 'M'
    
    def to_sql(self) -> tuple:
        if isinstance(self.sex, list):
            placeholders = ", ".join(["?" for _ in self.sex])
            return (f"p.sex IN ({placeholders})", self.sex)
        else:
            return ("p.sex = ?", [self.sex])


@dataclass
class DiagnosisCriterion(Criterion):
    """
    Filter patients by diagnosis codes.
    Supports ICD-9/10 codes, prefixes, and code hierarchies.
    
    Examples:
        DiagnosisCriterion(icd_codes=['E11.9', 'E11.65'])  # Exact codes
        DiagnosisCriterion(icd_prefix='E11')  # All codes starting with E11
        DiagnosisCriterion(icd_category='diabetes')  # Predefined category
    """
    icd_codes: Optional[List[str]] = None
    icd_prefix: Optional[str] = None
    icd_category: Optional[str] = None
    icd_version: Optional[int] = None
    temporal: Optional[TemporalRelation] = None
    temporal_days: Optional[int] = None
    
    # Predefined ICD categories
    CATEGORIES = {
        "diabetes": ["E11", "E10", "E13"],
        "hypertension": ["I10", "I11", "I12", "I13", "I15"],
        "cardiovascular": ["I20", "I21", "I22", "I23", "I24", "I25", "I48", "I50"],
        "respiratory": ["J40", "J41", "J42", "J43", "J44", "J18", "J45"],
        "mental_health": ["F32", "F33", "F41", "F10"],
        "musculoskeletal": ["M54", "M17", "M79"],
        "neoplasm": ["C34", "C50", "D44"],
        "kidney": ["N18", "N17", "N19"],
    }
    
    def __post_init__(self):
        if not any([self.icd_codes, self.icd_prefix, self.icd_category]):
            raise ValueError("At least one of icd_codes, icd_prefix, or icd_category must be specified")
    
    def to_sql(self) -> tuple:
        conditions = []
        params = []
        
        # Base condition for ICD version
        if self.icd_version is not None:
            conditions.append("d.icd_version = ?")
            params.append(self.icd_version)
        
        # ICD code matching
        if self.icd_codes:
            placeholders = ", ".join(["?" for _ in self.icd_codes])
            conditions.append(f"d.icd_code IN ({placeholders})")
            params.extend(self.icd_codes)
        
        # ICD prefix matching
        if self.icd_prefix:
            conditions.append("d.icd_code LIKE ?")
            params.append(f"{self.icd_prefix}%")
        
        # ICD category matching
        if self.icd_category:
            if self.icd_category in self.CATEGORIES:
                prefixes = self.CATEGORIES[self.icd_category]
                placeholders = ", ".join(["?" for _ in prefixes])
                conditions.append(f"d.icd_code LIKE ?")
                # Use OR for multiple prefixes
                prefix_conditions = " OR ".join([f"d.icd_code LIKE ?" for _ in prefixes])
                conditions = [c for c in conditions if "LIKE ?" not in c or "icd_code" not in c]
                conditions.append(f"({prefix_conditions})")
                params.extend([f"{p}%" for p in prefixes])
            else:
                raise ValueError(f"Unknown ICD category: {self.icd_category}")
        
        return (" AND ".join(conditions), params)


@dataclass
class MedicationCriterion(Criterion):
    """
    Filter patients by medication exposure.
    
    Examples:
        MedicationCriterion(medication_name='Metformin')
        MedicationCriterion(medication_names=['Aspirin', 'Clopidogrel'])
        MedicationCriterion(ndc_code='00093105601')
    """
    medication_name: Optional[str] = None
    medication_names: Optional[List[str]] = None
    ndc_code: Optional[str] = None
    start_date: Optional[str] = None
    end_date: Optional[str] = None
    within_days: Optional[int] = None
    
    def to_sql(self) -> tuple:
        conditions = []
        params = []
        
        # Medication name matching
        if self.medication_name:
            conditions.append("m.medication_name = ?")
            params.append(self.medication_name)
        
        if self.medication_names:
            placeholders = ", ".join(["?" for _ in self.medication_names])
            conditions.append(f"m.medication_name IN ({placeholders})")
            params.extend(self.medication_names)
        
        # NDC code matching
        if self.ndc_code:
            conditions.append("m.ndc_code = ?")
            params.append(self.ndc_code)
        
        # Date range
        if self.start_date:
            conditions.append("m.start_date >= ?")
            params.append(self.start_date)
        
        if self.end_date:
            conditions.append("m.start_date <= ?")
            params.append(self.end_date)
        
        # Within days of index date
        if self.within_days is not None:
            conditions.append("julianday('now') - julianday(m.start_date) <= ?")
            params.append(self.within_days)
        
        return (" AND ".join(conditions), params)


@dataclass
class LabCriterion(Criterion):
    """
    Filter patients by lab values.
    
    Examples:
        LabCriterion(lab_name='Glucose', min_value=126)
        LabCriterion(loinc_code='4548-4', min_value=6.5)  # HbA1c
        LabCriterion(lab_name='Glucose', min_value=200, abnormal_only=True)
    """
    lab_name: Optional[str] = None
    loinc_code: Optional[str] = None
    min_value: Optional[float] = None
    max_value: Optional[float] = None
    abnormal_only: bool = False
    within_days: Optional[int] = None
    
    def __post_init__(self):
        if not any([self.lab_name, self.loinc_code]):
            raise ValueError("At least one of lab_name or loinc_code must be specified")
        if self.min_value is None and self.max_value is None and not self.abnormal_only:
            raise ValueError("At least one of min_value, max_value, or abnormal_only must be specified")
    
    def to_sql(self) -> tuple:
        conditions = []
        params = []
        
        # Lab name matching
        if self.lab_name:
            conditions.append("l.lab_name = ?")
            params.append(self.lab_name)
        
        # LOINC code matching
        if self.loinc_code:
            conditions.append("l.loinc_code = ?")
            params.append(self.loinc_code)
        
        # Value thresholds
        if self.min_value is not None:
            conditions.append("l.result_value >= ?")
            params.append(self.min_value)
        
        if self.max_value is not None:
            conditions.append("l.result_value <= ?")
            params.append(self.max_value)
        
        # Abnormal flag
        if self.abnormal_only:
            conditions.append("l.abnormal_flag IN ('H', 'L')")
        
        # Within days
        if self.within_days is not None:
            conditions.append("julianday('now') - julianday(l.result_date) <= ?")
            params.append(self.within_days)
        
        return (" AND ".join(conditions), params)


@dataclass
class ProcedureCriterion(Criterion):
    """
    Filter patients by procedures.
    
    Examples:
        ProcedureCriterion(procedure_code='99213')
        ProcedureCriterion(procedure_name='Chest X-ray')
        ProcedureCriterion(cpt_code='71046')
    """
    procedure_code: Optional[str] = None
    procedure_name: Optional[str] = None
    cpt_code: Optional[str] = None
    
    def to_sql(self) -> tuple:
        conditions = []
        params = []
        
        if self.procedure_code:
            conditions.append("pr.procedure_code = ?")
            params.append(self.procedure_code)
        
        if self.procedure_name:
            conditions.append("pr.procedure_name LIKE ?")
            params.append(f"%{self.procedure_name}%")
        
        if self.cpt_code:
            conditions.append("pr.cpt_code = ?")
            params.append(self.cpt_code)
        
        return (" AND ".join(conditions), params)


@dataclass
class EncounterCriterion(Criterion):
    """
    Filter patients by encounter characteristics.
    
    Examples:
        EncounterCriterion(encounter_type='IP')
        EncounterCriterion(department='Cardiology')
        EncounterCriterion(min_encounters=3)
    """
    encounter_type: Optional[str] = None
    department: Optional[str] = None
    facility: Optional[str] = None
    min_encounters: Optional[int] = None
    max_encounters: Optional[int] = None
    start_date: Optional[str] = None
    end_date: Optional[str] = None
    
    def to_sql(self) -> tuple:
        conditions = []
        params = []
        
        if self.encounter_type:
            conditions.append("e.encounter_type = ?")
            params.append(self.encounter_type)
        
        if self.department:
            conditions.append("e.department = ?")
            params.append(self.department)
        
        if self.facility:
            conditions.append("e.facility = ?")
            params.append(self.facility)
        
        if self.start_date:
            conditions.append("e.encounter_date >= ?")
            params.append(self.start_date)
        
        if self.end_date:
            conditions.append("e.encounter_date <= ?")
            params.append(self.end_date)
        
        return (" AND ".join(conditions), params)


@dataclass
class CompoundCriterion(Criterion):
    """
    Combine multiple criteria with logical operators.
    
    Examples:
        CompoundCriterion(
            criteria=[AgeCriterion(min_age=18), SexCriterion(sex='M')],
            operator=LogicalOperator.AND
        )
        CompoundCriterion(
            criteria=[
                DiagnosisCriterion(icd_category='diabetes'),
                MedicationCriterion(medication_name='Metformin')
            ],
            operator=LogicalOperator.OR
        )
    """
    criteria: List[Criterion] = field(default_factory=list)
    operator: LogicalOperator = LogicalOperator.AND
    
    def to_sql(self) -> tuple:
        if not self.criteria:
            return ("1=1", [])
        
        all_conditions = []
        all_params = []
        
        for criterion in self.criteria:
            sql_clause, params = criterion.to_sql()
            if sql_clause:
                all_conditions.append(f"({sql_clause})")
                all_params.extend(params)
        
        combined = f" {self.operator.value} ".join(all_conditions)
        return (combined, all_params)


@dataclass
class TemporalCriterion(Criterion):
    """
    Filter patients based on temporal relationships between events.
    
    Examples:
        # Diabetes diagnosis within 30 days of encounter
        TemporalCriterion(
            diagnosis=DiagnosisCriterion(icd_category='diabetes'),
            encounter=EncounterCriterion(encounter_type='ED'),
            relation=TemporalRelation.WITHIN_DAYS,
            days=30
        )
    """
    diagnosis: Optional[DiagnosisCriterion] = None
    medication: Optional[MedicationCriterion] = None
    lab: Optional[LabCriterion] = None
    encounter: Optional[EncounterCriterion] = None
    relation: TemporalRelation = TemporalRelation.WITHIN_DAYS
    days: Optional[int] = None
    
    def to_sql(self) -> tuple:
        """
        Generate SQL for temporal relationship.
        This is more complex and requires subqueries.
        """
        # Build the first event condition
        first_conditions = []
        first_params = []
        
        if self.diagnosis:
            sql, params = self.diagnosis.to_sql()
            first_conditions.append(sql)
            first_params.extend(params)
        
        if self.medication:
            sql, params = self.medication.to_sql()
            first_conditions.append(sql)
            first_params.extend(params)
        
        if self.lab:
            sql, params = self.lab.to_sql()
            first_conditions.append(sql)
            first_params.extend(params)
        
        # Build the second event condition
        second_conditions = []
        second_params = []
        
        if self.encounter:
            sql, params = self.encounter.to_sql()
            second_conditions.append(sql)
            second_params.extend(params)
        
        # Combine with temporal relation
        first_sql = " AND ".join(first_conditions) if first_conditions else "1=1"
        second_sql = " AND ".join(second_conditions) if second_conditions else "1=1"
        
        # Generate temporal condition based on relation type
        if self.relation == TemporalRelation.WITHIN_DAYS:
            temporal_sql = f"""
                EXISTS (
                    SELECT 1 FROM diagnoses d1
                    JOIN encounters e1 ON d1.encounter_id = e1.encounter_id
                    WHERE d1.patient_id = p.patient_id
                    AND {first_sql}
                    AND EXISTS (
                        SELECT 1 FROM encounters e2
                        WHERE e2.patient_id = p.patient_id
                        AND {second_sql}
                        AND ABS(julianday(e1.encounter_date) - julianday(e2.encounter_date)) <= ?
                    )
                )
            """
            params = first_params + second_params + [self.days or 0]
        elif self.relation == TemporalRelation.BEFORE:
            temporal_sql = f"""
                EXISTS (
                    SELECT 1 FROM diagnoses d1
                    JOIN encounters e1 ON d1.encounter_id = e1.encounter_id
                    WHERE d1.patient_id = p.patient_id
                    AND {first_sql}
                    AND EXISTS (
                        SELECT 1 FROM encounters e2
                        WHERE e2.patient_id = p.patient_id
                        AND {second_sql}
                        AND e1.encounter_date < e2.encounter_date
                    )
                )
            """
            params = first_params + second_params
        elif self.relation == TemporalRelation.AFTER:
            temporal_sql = f"""
                EXISTS (
                    SELECT 1 FROM diagnoses d1
                    JOIN encounters e1 ON d1.encounter_id = e1.encounter_id
                    WHERE d1.patient_id = p.patient_id
                    AND {first_sql}
                    AND EXISTS (
                        SELECT 1 FROM encounters e2
                        WHERE e2.patient_id = p.patient_id
                        AND {second_sql}
                        AND e1.encounter_date > e2.encounter_date
                    )
                )
            """
            params = first_params + second_params
        elif self.relation == TemporalRelation.ON_SAME_DAY:
            temporal_sql = f"""
                EXISTS (
                    SELECT 1 FROM diagnoses d1
                    JOIN encounters e1 ON d1.encounter_id = e1.encounter_id
                    WHERE d1.patient_id = p.patient_id
                    AND {first_sql}
                    AND EXISTS (
                        SELECT 1 FROM encounters e2
                        WHERE e2.patient_id = p.patient_id
                        AND {second_sql}
                        AND e1.encounter_date = e2.encounter_date
                    )
                )
            """
            params = first_params + second_params
        else:
            raise ValueError(f"Unsupported temporal relation: {self.relation}")
        
        return (temporal_sql, params)


@dataclass
class CohortDefinition:
    """
    Complete cohort definition with inclusion and exclusion criteria.
    
    Examples:
        definition = CohortDefinition(
            name="Diabetic Patients",
            description="Patients with Type 2 diabetes",
            inclusion_criteria=[
                AgeCriterion(min_age=18),
                DiagnosisCriterion(icd_category='diabetes')
            ],
            exclusion_criteria=[
                DiagnosisCriterion(icd_codes=['E10.9']).exclude()
            ]
        )
    """
    name: str
    description: str = ""
    inclusion_criteria: List[Criterion] = field(default_factory=list)
    exclusion_criteria: List[Criterion] = field(default_factory=list)
    created_at: str = field(default_factory=lambda: datetime.now().isoformat())
    
    def add_inclusion(self, criterion: Criterion) -> 'CohortDefinition':
        """Add an inclusion criterion."""
        criterion.criterion_type = CriterionType.INCLUSION
        self.inclusion_criteria.append(criterion)
        return self
    
    def add_exclusion(self, criterion: Criterion) -> 'CohortDefinition':
        """Add an exclusion criterion."""
        criterion.criterion_type = CriterionType.EXCLUSION
        self.exclusion_criteria.append(criterion)
        return self
    
    def to_dict(self) -> dict:
        """Convert to dictionary for serialization."""
        return {
            "name": self.name,
            "description": self.description,
            "inclusion_criteria": [self._criterion_to_dict(c) for c in self.inclusion_criteria],
            "exclusion_criteria": [self._criterion_to_dict(c) for c in self.exclusion_criteria],
            "created_at": self.created_at
        }
    
    def _criterion_to_dict(self, criterion: Criterion) -> dict:
        """Convert a criterion to dictionary."""
        result = {
            "type": type(criterion).__name__,
            "criterion_type": criterion.criterion_type.value,
        }
        
        # Add all fields except criterion_type and description
        for key, value in criterion.__dict__.items():
            if key not in ["criterion_type", "description"]:
                if hasattr(value, 'value'):  # Enum
                    result[key] = value.value
                else:
                    result[key] = value
        
        return result
    
    @classmethod
    def from_dict(cls, data: dict) -> 'CohortDefinition':
        """Create from dictionary."""
        definition = cls(
            name=data["name"],
            description=data.get("description", ""),
            created_at=data.get("created_at", datetime.now().isoformat())
        )
        
        # Reconstruct criteria
        for criterion_data in data.get("inclusion_criteria", []):
            criterion = cls._dict_to_criterion(criterion_data)
            if criterion:
                definition.add_inclusion(criterion)
        
        for criterion_data in data.get("exclusion_criteria", []):
            criterion = cls._dict_to_criterion(criterion_data)
            if criterion:
                definition.add_exclusion(criterion)
        
        return definition
    
    @classmethod
    def _dict_to_criterion(cls, data: dict) -> Optional[Criterion]:
        """Convert dictionary to criterion."""
        criterion_type = data.get("type")
        
        # Remove type field
        params = {k: v for k, v in data.items() if k != "type"}
        
        # Convert criterion_type string back to enum
        if "criterion_type" in params:
            params["criterion_type"] = CriterionType(params["criterion_type"])
        
        # Convert enum fields
        for key, value in params.items():
            if isinstance(value, str) and key.endswith("_type") or key.endswith("_relation"):
                try:
                    params[key] = Enum(value)
                except ValueError:
                    pass
        
        # Create criterion
        criterion_classes = {
            "AgeCriterion": AgeCriterion,
            "SexCriterion": SexCriterion,
            "DiagnosisCriterion": DiagnosisCriterion,
            "MedicationCriterion": MedicationCriterion,
            "LabCriterion": LabCriterion,
            "ProcedureCriterion": ProcedureCriterion,
            "EncounterCriterion": EncounterCriterion,
            "CompoundCriterion": CompoundCriterion,
            "TemporalCriterion": TemporalCriterion,
        }
        
        if criterion_type in criterion_classes:
            try:
                return criterion_classes[criterion_type](**params)
            except Exception as e:
                print(f"Error creating criterion: {e}")
                return None
        
        return None
