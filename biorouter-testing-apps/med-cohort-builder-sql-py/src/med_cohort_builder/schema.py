"""
Schema definitions for the synthetic EHR database.
Defines tables for patients, encounters, diagnoses, medications, labs, and procedures.
"""

import sqlite3
from typing import List, Dict, Any


# Table definitions as SQL DDL
TABLE_DEFINITIONS = {
    "patients": """
        CREATE TABLE IF NOT EXISTS patients (
            patient_id INTEGER PRIMARY KEY,
            birth_date TEXT NOT NULL,
            death_date TEXT,
            sex TEXT CHECK(sex IN ('M', 'F', 'O')) NOT NULL,
            race TEXT,
            ethnicity TEXT,
            address_zip TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )
    """,
    
    "encounters": """
        CREATE TABLE IF NOT EXISTS encounters (
            encounter_id INTEGER PRIMARY KEY,
            patient_id INTEGER NOT NULL,
            encounter_date TEXT NOT NULL,
            encounter_type TEXT CHECK(encounter_type IN ('IP', 'OP', 'ED', 'AV')) NOT NULL,
            department TEXT,
            facility TEXT,
            FOREIGN KEY (patient_id) REFERENCES patients(patient_id)
        )
    """,
    
    "diagnoses": """
        CREATE TABLE IF NOT EXISTS diagnoses (
            diagnosis_id INTEGER PRIMARY KEY,
            encounter_id INTEGER NOT NULL,
            patient_id INTEGER NOT NULL,
            icd_code TEXT NOT NULL,
            icd_version INTEGER CHECK(icd_version IN (9, 10)) NOT NULL,
            diagnosis_date TEXT NOT NULL,
            sequence_number INTEGER DEFAULT 1,
            FOREIGN KEY (encounter_id) REFERENCES encounters(encounter_id),
            FOREIGN KEY (patient_id) REFERENCES patients(patient_id)
        )
    """,
    
    "medications": """
        CREATE TABLE IF NOT EXISTS medications (
            medication_id INTEGER PRIMARY KEY,
            patient_id INTEGER NOT NULL,
            encounter_id INTEGER,
            medication_name TEXT NOT NULL,
            ndc_code TEXT,
            start_date TEXT NOT NULL,
            end_date TEXT,
            dosage TEXT,
            route TEXT,
            FOREIGN KEY (patient_id) REFERENCES patients(patient_id),
            FOREIGN KEY (encounter_id) REFERENCES encounters(encounter_id)
        )
    """,
    
    "labs": """
        CREATE TABLE IF NOT EXISTS labs (
            lab_id INTEGER PRIMARY KEY,
            patient_id INTEGER NOT NULL,
            encounter_id INTEGER,
            lab_name TEXT NOT NULL,
            loinc_code TEXT,
            result_value REAL,
            result_unit TEXT,
            reference_range TEXT,
            abnormal_flag TEXT CHECK(abnormal_flag IN ('H', 'L', 'N', NULL)),
            result_date TEXT NOT NULL,
            FOREIGN KEY (patient_id) REFERENCES patients(patient_id),
            FOREIGN KEY (encounter_id) REFERENCES encounters(encounter_id)
        )
    """,
    
    "procedures": """
        CREATE TABLE IF NOT EXISTS procedures (
            procedure_id INTEGER PRIMARY KEY,
            encounter_id INTEGER NOT NULL,
            patient_id INTEGER NOT NULL,
            procedure_code TEXT NOT NULL,
            procedure_name TEXT NOT NULL,
            procedure_date TEXT NOT NULL,
            cpt_code TEXT,
            FOREIGN KEY (encounter_id) REFERENCES encounters(encounter_id),
            FOREIGN KEY (patient_id) REFERENCES patients(patient_id)
        )
    """,
    
    "icd_hierarchy": """
        CREATE TABLE IF NOT EXISTS icd_hierarchy (
            icd_code TEXT PRIMARY KEY,
            description TEXT NOT NULL,
            parent_code TEXT,
            chapter TEXT,
            block_start TEXT,
            block_end TEXT
        )
    """
}


def create_database(db_path: str) -> None:
    """
    Create a new SQLite database with the EHR schema.
    
    Args:
        db_path: Path to the SQLite database file
    """
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    
    try:
        for table_name, ddl in TABLE_DEFINITIONS.items():
            cursor.execute(ddl)
        
        # Create indexes for better query performance
        indexes = [
            "CREATE INDEX IF NOT EXISTS idx_encounters_patient ON encounters(patient_id)",
            "CREATE INDEX IF NOT EXISTS idx_encounters_date ON encounters(encounter_date)",
            "CREATE INDEX IF NOT EXISTS idx_diagnoses_patient ON diagnoses(patient_id)",
            "CREATE INDEX IF NOT EXISTS idx_diagnoses_icd ON diagnoses(icd_code)",
            "CREATE INDEX IF NOT EXISTS idx_medications_patient ON medications(patient_id)",
            "CREATE INDEX IF NOT EXISTS idx_medications_name ON medications(medication_name)",
            "CREATE INDEX IF NOT EXISTS idx_labs_patient ON labs(patient_id)",
            "CREATE INDEX IF NOT EXISTS idx_labs_loinc ON labs(loinc_code)",
            "CREATE INDEX IF NOT EXISTS idx_procedures_patient ON procedures(patient_id)",
            "CREATE INDEX IF NOT EXISTS idx_procedures_code ON procedures(procedure_code)",
        ]
        
        for index_sql in indexes:
            cursor.execute(index_sql)
        
        conn.commit()
        
    except Exception as e:
        conn.rollback()
        raise e
    finally:
        conn.close()


def get_schema_info(db_path: str) -> Dict[str, List[str]]:
    """
    Get information about the database schema.
    
    Args:
        db_path: Path to the SQLite database file
        
    Returns:
        Dictionary mapping table names to their column names
    """
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    
    schema_info = {}
    
    try:
        # Get all table names
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        tables = cursor.fetchall()
        
        for (table_name,) in tables:
            cursor.execute(f"PRAGMA table_info({table_name})")
            columns = [row[1] for row in cursor.fetchall()]
            schema_info[table_name] = columns
            
    finally:
        conn.close()
    
    return schema_info


def drop_database(db_path: str) -> None:
    """
    Drop all tables from the database.
    
    Args:
        db_path: Path to the SQLite database file
    """
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    
    try:
        # Get all table names
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        tables = cursor.fetchall()
        
        for (table_name,) in tables:
            cursor.execute(f"DROP TABLE IF EXISTS {table_name}")
        
        conn.commit()
        
    finally:
        conn.close()
