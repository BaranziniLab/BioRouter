"""
Cohort summary statistics.
Provides functions to calculate summary statistics for patient cohorts.
"""

import sqlite3
from typing import List, Dict, Any, Optional
from dataclasses import dataclass, field
from datetime import datetime


@dataclass
class CohortSummary:
    """
    Summary statistics for a patient cohort.
    """
    cohort_name: str
    total_patients: int
    age_distribution: Dict[str, int] = field(default_factory=dict)
    sex_distribution: Dict[str, int] = field(default_factory=dict)
    race_distribution: Dict[str, int] = field(default_factory=dict)
    ethnicity_distribution: Dict[str, int] = field(default_factory=dict)
    top_diagnoses: List[Dict[str, Any]] = field(default_factory=list)
    top_medications: List[Dict[str, Any]] = field(default_factory=list)
    encounter_stats: Dict[str, Any] = field(default_factory=dict)
    lab_stats: Dict[str, Any] = field(default_factory=dict)
    mortality_rate: float = 0.0
    created_at: str = field(default_factory=lambda: datetime.now().isoformat())
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return {
            "cohort_name": self.cohort_name,
            "total_patients": self.total_patients,
            "age_distribution": self.age_distribution,
            "sex_distribution": self.sex_distribution,
            "race_distribution": self.race_distribution,
            "ethnicity_distribution": self.ethnicity_distribution,
            "top_diagnoses": self.top_diagnoses,
            "top_medications": self.top_medications,
            "encounter_stats": self.encounter_stats,
            "lab_stats": self.lab_stats,
            "mortality_rate": self.mortality_rate,
            "created_at": self.created_at
        }
    
    def print_summary(self) -> None:
        """Print a formatted summary to console."""
        print(f"\n{'='*60}")
        print(f"Cohort Summary: {self.cohort_name}")
        print(f"{'='*60}")
        print(f"\nTotal Patients: {self.total_patients:,}")
        print(f"Mortality Rate: {self.mortality_rate:.1%}")
        
        print(f"\n--- Age Distribution ---")
        for age_group, count in sorted(self.age_distribution.items()):
            pct = count / self.total_patients * 100 if self.total_patients > 0 else 0
            print(f"  {age_group}: {count:,} ({pct:.1f}%)")
        
        print(f"\n--- Sex Distribution ---")
        for sex, count in sorted(self.sex_distribution.items()):
            pct = count / self.total_patients * 100 if self.total_patients > 0 else 0
            print(f"  {sex}: {count:,} ({pct:.1f}%)")
        
        print(f"\n--- Top 10 Diagnoses ---")
        for i, diag in enumerate(self.top_diagnoses[:10], 1):
            print(f"  {i}. {diag['icd_code']} - {diag['description']}: {diag['patient_count']:,} patients")
        
        print(f"\n--- Top 10 Medications ---")
        for i, med in enumerate(self.top_medications[:10], 1):
            print(f"  {i}. {med['medication_name']}: {med['patient_count']:,} patients")
        
        print(f"\n--- Encounter Statistics ---")
        print(f"  Total Encounters: {self.encounter_stats.get('total_encounters', 0):,}")
        print(f"  Avg Encounters/Patient: {self.encounter_stats.get('avg_encounters_per_patient', 0):.1f}")
        
        print(f"{'='*60}\n")


class CohortSummarizer:
    """
    Generates summary statistics for patient cohorts.
    """
    
    def __init__(self, db_path: str):
        """
        Initialize the summarizer.
        
        Args:
            db_path: Path to the SQLite database
        """
        self.db_path = db_path
    
    def summarize(
        self, 
        patient_ids: List[int], 
        cohort_name: str = "Cohort"
    ) -> CohortSummary:
        """
        Generate summary statistics for a cohort.
        
        Args:
            patient_ids: List of patient IDs in the cohort
            cohort_name: Name of the cohort
            
        Returns:
            CohortSummary object with statistics
        """
        if not patient_ids:
            return CohortSummary(
                cohort_name=cohort_name,
                total_patients=0
            )
        
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        try:
            # Create placeholders for IN clause
            placeholders = ", ".join(["?" for _ in patient_ids])
            
            # Basic demographics
            cursor.execute(f"""
                SELECT COUNT(*) as total,
                       SUM(CASE WHEN death_date IS NOT NULL THEN 1 ELSE 0 END) as deceased
                FROM patients 
                WHERE patient_id IN ({placeholders})
            """, patient_ids)
            total, deceased = cursor.fetchone()
            
            mortality_rate = deceased / total if total > 0 else 0
            
            # Age distribution
            cursor.execute(f"""
                SELECT 
                    CASE 
                        WHEN (julianday('now') - julianday(birth_date)) / 365.25 < 18 THEN '0-17'
                        WHEN (julianday('now') - julianday(birth_date)) / 365.25 < 30 THEN '18-29'
                        WHEN (julianday('now') - julianday(birth_date)) / 365.25 < 40 THEN '30-39'
                        WHEN (julianday('now') - julianday(birth_date)) / 365.25 < 50 THEN '40-49'
                        WHEN (julianday('now') - julianday(birth_date)) / 365.25 < 60 THEN '50-59'
                        WHEN (julianday('now') - julianday(birth_date)) / 365.25 < 70 THEN '60-69'
                        WHEN (julianday('now') - julianday(birth_date)) / 365.25 < 80 THEN '70-79'
                        ELSE '80+'
                    END as age_group,
                    COUNT(*) as count
                FROM patients 
                WHERE patient_id IN ({placeholders})
                GROUP BY age_group
                ORDER BY age_group
            """, patient_ids)
            age_distribution = {row[0]: row[1] for row in cursor.fetchall()}
            
            # Sex distribution
            cursor.execute(f"""
                SELECT sex, COUNT(*) as count
                FROM patients 
                WHERE patient_id IN ({placeholders})
                GROUP BY sex
                ORDER BY sex
            """, patient_ids)
            sex_distribution = {row[0]: row[1] for row in cursor.fetchall()}
            
            # Race distribution
            cursor.execute(f"""
                SELECT race, COUNT(*) as count
                FROM patients 
                WHERE patient_id IN ({placeholders})
                GROUP BY race
                ORDER BY count DESC
            """, patient_ids)
            race_distribution = {row[0]: row[1] for row in cursor.fetchall()}
            
            # Ethnicity distribution
            cursor.execute(f"""
                SELECT ethnicity, COUNT(*) as count
                FROM patients 
                WHERE patient_id IN ({placeholders})
                GROUP BY ethnicity
                ORDER BY count DESC
            """, patient_ids)
            ethnicity_distribution = {row[0]: row[1] for row in cursor.fetchall()}
            
            # Top diagnoses
            cursor.execute(f"""
                SELECT d.icd_code, 
                       COUNT(DISTINCT d.patient_id) as patient_count,
                       COUNT(*) as total_mentions
                FROM diagnoses d
                WHERE d.patient_id IN ({placeholders})
                GROUP BY d.icd_code
                ORDER BY patient_count DESC
                LIMIT 20
            """, patient_ids)
            
            top_diagnoses = []
            for row in cursor.fetchall():
                # Get description from ICD hierarchy or use code
                cursor.execute(
                    "SELECT description FROM icd_hierarchy WHERE icd_code = ?",
                    (row[0],)
                )
                desc_row = cursor.fetchone()
                description = desc_row[0] if desc_row else f"ICD Code {row[0]}"
                
                top_diagnoses.append({
                    "icd_code": row[0],
                    "description": description,
                    "patient_count": row[1],
                    "total_mentions": row[2]
                })
            
            # Top medications
            cursor.execute(f"""
                SELECT m.medication_name, 
                       COUNT(DISTINCT m.patient_id) as patient_count,
                       COUNT(*) as total_prescriptions
                FROM medications m
                WHERE m.patient_id IN ({placeholders})
                GROUP BY m.medication_name
                ORDER BY patient_count DESC
                LIMIT 20
            """, patient_ids)
            
            top_medications = [
                {
                    "medication_name": row[0],
                    "patient_count": row[1],
                    "total_prescriptions": row[2]
                }
                for row in cursor.fetchall()
            ]
            
            # Encounter statistics
            cursor.execute(f"""
                SELECT COUNT(*) as total_encounters,
                       AVG(encounters_per_patient) as avg_encounters
                FROM (
                    SELECT patient_id, COUNT(*) as encounters_per_patient
                    FROM encounters
                    WHERE patient_id IN ({placeholders})
                    GROUP BY patient_id
                )
            """, patient_ids)
            
            enc_stats = cursor.fetchone()
            encounter_stats = {
                "total_encounters": enc_stats[0],
                "avg_encounters_per_patient": round(enc_stats[1], 1) if enc_stats[1] else 0
            }
            
            # Lab statistics
            cursor.execute(f"""
                SELECT COUNT(DISTINCT l.patient_id) as patients_with_labs,
                       AVG(l.result_value) as avg_value,
                       MIN(l.result_value) as min_value,
                       MAX(l.result_value) as max_value
                FROM labs l
                WHERE l.patient_id IN ({placeholders})
            """, patient_ids)
            
            lab_stats_row = cursor.fetchone()
            lab_stats = {
                "patients_with_labs": lab_stats_row[0],
                "avg_value": round(lab_stats_row[1], 2) if lab_stats_row[1] else None,
                "min_value": lab_stats_row[2],
                "max_value": lab_stats_row[3]
            }
            
            return CohortSummary(
                cohort_name=cohort_name,
                total_patients=total,
                age_distribution=age_distribution,
                sex_distribution=sex_distribution,
                race_distribution=race_distribution,
                ethnicity_distribution=ethnicity_distribution,
                top_diagnoses=top_diagnoses,
                top_medications=top_medications,
                encounter_stats=encounter_stats,
                lab_stats=lab_stats,
                mortality_rate=mortality_rate
            )
            
        finally:
            conn.close()
