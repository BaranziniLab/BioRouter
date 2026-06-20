"""
Incidence and prevalence calculator.
Provides functions to calculate point prevalence, period prevalence, and incidence rates.
"""

import sqlite3
from typing import List, Dict, Any, Optional, Tuple
from dataclasses import dataclass
from datetime import datetime, timedelta
from enum import Enum


class PrevalenceType(Enum):
    """Types of prevalence measures."""
    POINT_PREVALENCE = "point_prevalence"
    PERIOD_PREVALENCE = "period_prevalence"
    INCIDENCE_RATE = "incidence_rate"
    CUMULATIVE_INCIDENCE = "cumulative_incidence"


@dataclass
class PrevalenceResult:
    """
    Results from prevalence/incidence calculation.
    """
    measure_type: PrevalenceType
    numerator: int  # Cases
    denominator: int  # Population at risk
    rate: float  # Calculated rate (per 1000 or proportion)
    rate_per: int  # Rate denominator (e.g., 1000 for per 1000)
    period_start: Optional[str] = None
    period_end: Optional[str] = None
    description: str = ""
    
    @property
    def proportion(self) -> float:
        """Get as proportion (0-1)."""
        return self.numerator / self.denominator if self.denominator > 0 else 0
    
    @property
    def percentage(self) -> float:
        """Get as percentage."""
        return self.proportion * 100
    
    @property
    def per_thousand(self) -> float:
        """Get rate per 1000."""
        return (self.numerator / self.denominator * 1000) if self.denominator > 0 else 0
    
    @property
    def per_100000(self) -> float:
        """Get rate per 100,000."""
        return (self.numerator / self.denominator * 100000) if self.denominator > 0 else 0
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return {
            "measure_type": self.measure_type.value,
            "numerator": self.numerator,
            "denominator": self.denominator,
            "rate": self.rate,
            "rate_per": self.rate_per,
            "proportion": self.proportion,
            "percentage": self.percentage,
            "per_thousand": self.per_thousand,
            "per_100000": self.per_100000,
            "period_start": self.period_start,
            "period_end": self.period_end,
            "description": self.description
        }
    
    def __str__(self) -> str:
        """String representation."""
        if self.measure_type == PrevalenceType.INCIDENCE_RATE:
            return (
                f"Incidence Rate: {self.numerator}/{self.denominator} "
                f"= {self.per_thousand:.2f} per 1,000 person-years"
            )
        else:
            return (
                f"Prevalence: {self.numerator}/{self.denominator} "
                f"= {self.percentage:.2f}% ({self.per_thousand:.2f} per 1,000)"
            )


class PrevalenceCalculator:
    """
    Calculates incidence and prevalence measures.
    """
    
    def __init__(self, db_path: str):
        """
        Initialize the calculator.
        
        Args:
            db_path: Path to the SQLite database
        """
        self.db_path = db_path
    
    def point_prevalence(
        self,
        patient_ids: List[int],
        condition_sql: str,
        condition_params: List[Any],
        prevalence_date: str,
        description: str = "Point Prevalence"
    ) -> PrevalenceResult:
        """
        Calculate point prevalence at a specific date.
        
        Args:
            patient_ids: List of patient IDs in the population
            condition_sql: SQL condition for the disease/condition
            condition_params: Parameters for the condition SQL
            prevalence_date: Date to calculate prevalence (YYYY-MM-DD)
            description: Description of the measure
            
        Returns:
            PrevalenceResult with the calculated prevalence
        """
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        try:
            placeholders = ", ".join(["?" for _ in patient_ids])
            
            # Count cases (patients with condition at prevalence date)
            case_sql = f"""
                SELECT COUNT(DISTINCT p.patient_id)
                FROM patients p
                WHERE p.patient_id IN ({placeholders})
                AND ({condition_sql})
                AND p.birth_date <= ?
                AND (p.death_date IS NULL OR p.death_date >= ?)
            """
            cursor.execute(case_sql, patient_ids + condition_params + [prevalence_date, prevalence_date])
            cases = cursor.fetchone()[0]
            
            # Count total population alive at prevalence date
            pop_sql = f"""
                SELECT COUNT(*)
                FROM patients p
                WHERE p.patient_id IN ({placeholders})
                AND p.birth_date <= ?
                AND (p.death_date IS NULL OR p.death_date >= ?)
            """
            cursor.execute(pop_sql, patient_ids + [prevalence_date, prevalence_date])
            population = cursor.fetchone()[0]
            
            rate = cases / population if population > 0 else 0
            
            return PrevalenceResult(
                measure_type=PrevalenceType.POINT_PREVALENCE,
                numerator=cases,
                denominator=population,
                rate=rate,
                rate_per=1000,
                period_start=prevalence_date,
                period_end=prevalence_date,
                description=description
            )
            
        finally:
            conn.close()
    
    def period_prevalence(
        self,
        patient_ids: List[int],
        condition_sql: str,
        condition_params: List[Any],
        start_date: str,
        end_date: str,
        description: str = "Period Prevalence"
    ) -> PrevalenceResult:
        """
        Calculate period prevalence over a time period.
        
        Args:
            patient_ids: List of patient IDs in the population
            condition_sql: SQL condition for the disease/condition
            condition_params: Parameters for the condition SQL
            start_date: Start of the period (YYYY-MM-DD)
            end_date: End of the period (YYYY-MM-DD)
            description: Description of the measure
            
        Returns:
            PrevalenceResult with the calculated prevalence
        """
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        try:
            placeholders = ", ".join(["?" for _ in patient_ids])
            
            # Count cases (patients with condition during period)
            case_sql = f"""
                SELECT COUNT(DISTINCT p.patient_id)
                FROM patients p
                WHERE p.patient_id IN ({placeholders})
                AND ({condition_sql})
                AND p.birth_date <= ?
                AND (p.death_date IS NULL OR p.death_date >= ?)
            """
            cursor.execute(case_sql, patient_ids + condition_params + [end_date, start_date])
            cases = cursor.fetchone()[0]
            
            # Count population alive at any point during period
            pop_sql = f"""
                SELECT COUNT(*)
                FROM patients p
                WHERE p.patient_id IN ({placeholders})
                AND p.birth_date <= ?
                AND (p.death_date IS NULL OR p.death_date >= ?)
            """
            cursor.execute(pop_sql, patient_ids + [end_date, start_date])
            population = cursor.fetchone()[0]
            
            rate = cases / population if population > 0 else 0
            
            return PrevalenceResult(
                measure_type=PrevalenceType.PERIOD_PREVALENCE,
                numerator=cases,
                denominator=population,
                rate=rate,
                rate_per=1000,
                period_start=start_date,
                period_end=end_date,
                description=description
            )
            
        finally:
            conn.close()
    
    def incidence_rate(
        self,
        patient_ids: List[int],
        condition_sql: str,
        condition_params: List[Any],
        start_date: str,
        end_date: str,
        description: str = "Incidence Rate"
    ) -> PrevalenceResult:
        """
        Calculate incidence rate (new cases per person-time).
        
        Args:
            patient_ids: List of patient IDs in the population
            condition_sql: SQL condition for the disease/condition
            condition_params: Parameters for the condition SQL
            start_date: Start of observation period (YYYY-MM-DD)
            end_date: End of observation period (YYYY-MM-DD)
            description: Description of the measure
            
        Returns:
            PrevalenceResult with the calculated incidence rate
        """
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        try:
            placeholders = ", ".join(["?" for _ in patient_ids])
            
            # Count new cases during period
            case_sql = f"""
                SELECT COUNT(DISTINCT p.patient_id)
                FROM patients p
                WHERE p.patient_id IN ({placeholders})
                AND ({condition_sql})
                AND p.birth_date <= ?
                AND (p.death_date IS NULL OR p.death_date >= ?)
            """
            cursor.execute(case_sql, patient_ids + condition_params + [end_date, start_date])
            cases = cursor.fetchone()[0]
            
            # Calculate person-time at risk (in years)
            # For simplicity, assume uniform observation period
            pop_sql = f"""
                SELECT COUNT(*)
                FROM patients p
                WHERE p.patient_id IN ({placeholders})
                AND p.birth_date <= ?
                AND (p.death_date IS NULL OR p.death_date >= ?)
            """
            cursor.execute(pop_sql, patient_ids + [end_date, start_date])
            population = cursor.fetchone()[0]
            
            # Calculate person-years (simplified)
            start_dt = datetime.strptime(start_date, "%Y-%m-%d")
            end_dt = datetime.strptime(end_date, "%Y-%m-%d")
            years = (end_dt - start_dt).days / 365.25
            person_years = population * years
            
            # Incidence rate per 1000 person-years
            rate = cases / person_years * 1000 if person_years > 0 else 0
            
            return PrevalenceResult(
                measure_type=PrevalenceType.INCIDENCE_RATE,
                numerator=cases,
                denominator=population,
                rate=rate,
                rate_per=1000,
                period_start=start_date,
                period_end=end_date,
                description=description
            )
            
        finally:
            conn.close()
    
    def cumulative_incidence(
        self,
        patient_ids: List[int],
        condition_sql: str,
        condition_params: List[Any],
        start_date: str,
        end_date: str,
        description: str = "Cumulative Incidence"
    ) -> PrevalenceResult:
        """
        Calculate cumulative incidence (risk) over a period.
        
        Args:
            patient_ids: List of patient IDs in the population
            condition_sql: SQL condition for the disease/condition
            condition_params: Parameters for the condition SQL
            start_date: Start of observation period (YYYY-MM-DD)
            end_date: End of observation period (YYYY-MM-DD)
            description: Description of the measure
            
        Returns:
            PrevalenceResult with the calculated cumulative incidence
        """
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        try:
            placeholders = ", ".join(["?" for _ in patient_ids])
            
            # Count new cases during period
            case_sql = f"""
                SELECT COUNT(DISTINCT p.patient_id)
                FROM patients p
                WHERE p.patient_id IN ({placeholders})
                AND ({condition_sql})
                AND p.birth_date <= ?
                AND (p.death_date IS NULL OR p.death_date >= ?)
            """
            cursor.execute(case_sql, patient_ids + condition_params + [end_date, start_date])
            cases = cursor.fetchone()[0]
            
            # Count population at risk at start
            pop_sql = f"""
                SELECT COUNT(*)
                FROM patients p
                WHERE p.patient_id IN ({placeholders})
                AND p.birth_date <= ?
                AND (p.death_date IS NULL OR p.death_date >= ?)
            """
            cursor.execute(pop_sql, patient_ids + [start_date, start_date])
            population = cursor.fetchone()[0]
            
            rate = cases / population if population > 0 else 0
            
            return PrevalenceResult(
                measure_type=PrevalenceType.CUMULATIVE_INCIDENCE,
                numerator=cases,
                denominator=population,
                rate=rate,
                rate_per=1000,
                period_start=start_date,
                period_end=end_date,
                description=description
            )
            
        finally:
            conn.close()
    
    def calculate_diagnosis_prevalence(
        self,
        patient_ids: List[int],
        icd_codes: Optional[List[str]] = None,
        icd_prefix: Optional[str] = None,
        prevalence_date: Optional[str] = None,
        start_date: Optional[str] = None,
        end_date: Optional[str] = None
    ) -> PrevalenceResult:
        """
        Convenience method to calculate diagnosis prevalence.
        
        Args:
            patient_ids: List of patient IDs
            icd_codes: List of ICD codes
            icd_prefix: ICD code prefix
            prevalence_date: For point prevalence
            start_date: For period prevalence
            end_date: For period prevalence
            
        Returns:
            PrevalenceResult
        """
        # Build condition SQL
        conditions = []
        params = []
        
        if icd_codes:
            placeholders = ", ".join(["?" for _ in icd_codes])
            conditions.append(f"d.icd_code IN ({placeholders})")
            params.extend(icd_codes)
        
        if icd_prefix:
            conditions.append("d.icd_code LIKE ?")
            params.append(f"{icd_prefix}%")
        
        condition_sql = " AND ".join(conditions) if conditions else "1=1"
        
        if prevalence_date:
            return self.point_prevalence(
                patient_ids, condition_sql, params, prevalence_date,
                f"Point prevalence of ICD codes"
            )
        elif start_date and end_date:
            return self.period_prevalence(
                patient_ids, condition_sql, params, start_date, end_date,
                f"Period prevalence of ICD codes"
            )
        else:
            raise ValueError("Either prevalence_date or start_date/end_date must be provided")
