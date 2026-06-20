"""
SQL compiler for cohort criteria.
Converts cohort definitions into parameterized SQL queries.
"""

import sqlite3
from typing import List, Tuple, Dict, Any, Optional
from dataclasses import dataclass

from .criteria import (
    Criterion, CohortDefinition, CriterionType, 
    CompoundCriterion, LogicalOperator
)


@dataclass
class SQLQuery:
    """
    Represents a compiled SQL query with parameters.
    """
    sql: str
    params: List[Any]
    cohort_name: str
    description: str
    
    def __str__(self) -> str:
        return f"-- {self.cohort_name}\n{self.sql}\n-- Parameters: {self.params}"


class SQLCompiler:
    """
    Compiles cohort definitions to parameterized SQL queries.
    """
    
    # Base query templates
    BASE_QUERY = """
    SELECT DISTINCT p.patient_id
    FROM patients p
    WHERE {where_clause}
    """
    
    PATIENT_DIAGNOSIS_EXISTS = """
    EXISTS (
        SELECT 1 FROM diagnoses d 
        WHERE d.patient_id = p.patient_id
        AND {conditions}
    )
    """
    
    PATIENT_MEDICATION_EXISTS = """
    EXISTS (
        SELECT 1 FROM medications m 
        WHERE m.patient_id = p.patient_id
        AND {conditions}
    )
    """
    
    PATIENT_LAB_EXISTS = """
    EXISTS (
        SELECT 1 FROM labs l 
        WHERE l.patient_id = p.patient_id
        AND {conditions}
    )
    """
    
    PATIENT_PROCEDURE_EXISTS = """
    EXISTS (
        SELECT 1 FROM procedures pr 
        WHERE pr.patient_id = p.patient_id
        AND {conditions}
    )
    """
    
    PATIENT_ENCOUNTER_EXISTS = """
    EXISTS (
        SELECT 1 FROM encounters e 
        WHERE e.patient_id = p.patient_id
        AND {conditions}
    )
    """
    
    PATIENT_ENCOUNTER_COUNT = """
    (SELECT COUNT(*) FROM encounters e 
     WHERE e.patient_id = p.patient_id
     AND {conditions}) >= ?
    """
    
    def __init__(self, db_path: str):
        """
        Initialize the compiler with a database path.
        
        Args:
            db_path: Path to the SQLite database
        """
        self.db_path = db_path
    
    def compile(self, definition: CohortDefinition) -> SQLQuery:
        """
        Compile a cohort definition to SQL.
        
        Args:
            definition: The cohort definition to compile
            
        Returns:
            SQLQuery object with the compiled SQL and parameters
        """
        all_conditions = []
        all_params = []
        
        # Process inclusion criteria
        if definition.inclusion_criteria:
            inclusion_sql, inclusion_params = self._compile_criteria(
                definition.inclusion_criteria, LogicalOperator.AND
            )
            if inclusion_sql:
                all_conditions.append(f"({inclusion_sql})")
                all_params.extend(inclusion_params)
        
        # Process exclusion criteria
        if definition.exclusion_criteria:
            # Exclusion criteria are applied as NOT (wrapped in EXISTS)
            for criterion in definition.exclusion_criteria:
                sql, params = criterion.to_sql()
                if sql:
                    wrapped_sql = self._wrap_condition(criterion, sql)
                    all_conditions.append(f"NOT ({wrapped_sql})")
                    all_params.extend(params)
        
        # Build final WHERE clause
        where_clause = " AND ".join(all_conditions) if all_conditions else "1=1"
        
        # Build final query
        sql = self.BASE_QUERY.format(where_clause=where_clause)
        
        return SQLQuery(
            sql=sql,
            params=all_params,
            cohort_name=definition.name,
            description=definition.description
        )
    
    def _compile_criteria(
        self, 
        criteria: List[Criterion], 
        operator: LogicalOperator
    ) -> Tuple[str, List[Any]]:
        """
        Compile a list of criteria with a logical operator.
        
        Args:
            criteria: List of criteria to compile
            operator: Logical operator (AND/OR)
            
        Returns:
            Tuple of (sql_clause, parameters)
        """
        if not criteria:
            return ("1=1", [])
        
        conditions = []
        params = []
        
        for criterion in criteria:
            # Handle compound criteria recursively
            if isinstance(criterion, CompoundCriterion):
                sql, criterion_params = self._compile_criteria(
                    criterion.criteria, criterion.operator
                )
                if sql:
                    conditions.append(f"({sql})")
                    params.extend(criterion_params)
            else:
                sql, criterion_params = criterion.to_sql()
                if sql:
                    # Wrap complex conditions in EXISTS
                    wrapped_sql = self._wrap_condition(criterion, sql)
                    conditions.append(f"({wrapped_sql})")
                    params.extend(criterion_params)
        
        combined = f" {operator.value} ".join(conditions)
        return (combined, params)
    
    def _wrap_condition(self, criterion: Criterion, sql: str) -> str:
        """
        Wrap a condition with appropriate EXISTS clause if needed.
        
        Args:
            criterion: The criterion being wrapped
            sql: The SQL condition
            
        Returns:
            Wrapped SQL condition
        """
        # Import criterion types
        from .criteria import (
            DiagnosisCriterion, MedicationCriterion, 
            LabCriterion, ProcedureCriterion, EncounterCriterion
        )
        
        if isinstance(criterion, DiagnosisCriterion):
            return self.PATIENT_DIAGNOSIS_EXISTS.format(conditions=sql)
        elif isinstance(criterion, MedicationCriterion):
            return self.PATIENT_MEDICATION_EXISTS.format(conditions=sql)
        elif isinstance(criterion, LabCriterion):
            return self.PATIENT_LAB_EXISTS.format(conditions=sql)
        elif isinstance(criterion, ProcedureCriterion):
            return self.PATIENT_PROCEDURE_EXISTS.format(conditions=sql)
        elif isinstance(criterion, EncounterCriterion):
            if criterion.min_encounters:
                return self.PATIENT_ENCOUNTER_COUNT.format(conditions=sql)
            return self.PATIENT_ENCOUNTER_EXISTS.format(conditions=sql)
        else:
            return sql
    
    def execute(self, query: SQLQuery) -> List[int]:
        """
        Execute a compiled SQL query and return patient IDs.
        
        Args:
            query: The SQLQuery to execute
            
        Returns:
            List of patient IDs matching the criteria
        """
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        try:
            cursor.execute(query.sql, query.params)
            results = cursor.fetchall()
            return [row[0] for row in results]
        finally:
            conn.close()
    
    def get_cohort_size(self, query: SQLQuery) -> int:
        """
        Get the size of a cohort without retrieving all patient IDs.
        
        Args:
            query: The SQLQuery to execute
            
        Returns:
            Number of patients in the cohort
        """
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        try:
            # Modify query to count instead of selecting IDs
            count_sql = f"SELECT COUNT(*) FROM ({query.sql})"
            cursor.execute(count_sql, query.params)
            result = cursor.fetchone()
            return result[0] if result else 0
        finally:
            conn.close()


class CohortQueryBuilder:
    """
    Fluent builder for constructing cohort queries.
    """
    
    def __init__(self, db_path: str):
        """
        Initialize the builder.
        
        Args:
            db_path: Path to the SQLite database
        """
        self.db_path = db_path
        self.compiler = SQLCompiler(db_path)
        self.definition = CohortDefinition(name="Unnamed Cohort")
    
    def set_name(self, name: str) -> 'CohortQueryBuilder':
        """Set the cohort name."""
        self.definition.name = name
        return self
    
    def set_description(self, description: str) -> 'CohortQueryBuilder':
        """Set the cohort description."""
        self.definition.description = description
        return self
    
    def include(self, criterion: Criterion) -> 'CohortQueryBuilder':
        """Add an inclusion criterion."""
        self.definition.add_inclusion(criterion)
        return self
    
    def exclude(self, criterion: Criterion) -> 'CohortQueryBuilder':
        """Add an exclusion criterion."""
        self.definition.add_exclusion(criterion)
        return self
    
    def build(self) -> SQLQuery:
        """Build and return the SQL query."""
        return self.compiler.compile(self.definition)
    
    def execute(self) -> List[int]:
        """Build and execute the query, returning patient IDs."""
        query = self.build()
        return self.compiler.execute(query)
    
    def get_size(self) -> int:
        """Get the cohort size."""
        query = self.build()
        return self.compiler.get_cohort_size(query)
