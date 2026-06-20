"""
Synthetic EHR data generator.
Creates realistic-ish synthetic records for patients, encounters, diagnoses, medications, labs, and procedures.
"""

import sqlite3
import random
from datetime import datetime, timedelta
from typing import List, Tuple, Optional
import json


# Common ICD-10 codes for realistic data generation
ICD10_CATEGORIES = {
    "diabetes": [
        ("E11.9", "Type 2 diabetes mellitus without complications"),
        ("E11.65", "Type 2 diabetes mellitus with hyperglycemia"),
        ("E10.9", "Type 1 diabetes mellitus without complications"),
    ],
    "hypertension": [
        ("I10", "Essential (primary) hypertension"),
        ("I11.9", "Hypertensive heart disease without heart failure"),
    ],
    "cardiovascular": [
        ("I21.9", "Acute myocardial infarction, unspecified"),
        ("I25.10", "Atherosclerotic heart disease of native coronary artery"),
        ("I48.91", "Unspecified atrial fibrillation"),
        ("I50.9", "Heart failure, unspecified"),
    ],
    "respiratory": [
        ("J44.1", "Chronic obstructive pulmonary disease with acute exacerbation"),
        ("J18.9", "Pneumonia, unspecified organism"),
        ("J45.909", "Unspecified asthma, uncomplicated"),
    ],
    "mental_health": [
        ("F32.9", "Major depressive disorder, single episode, unspecified"),
        ("F41.1", "Generalized anxiety disorder"),
        ("F10.20", "Alcohol dependence, uncomplicated"),
    ],
    "musculoskeletal": [
        ("M54.5", "Low back pain"),
        ("M17.11", "Primary osteoarthritis, right knee"),
        ("M79.3", "Panniculitis, unspecified"),
    ],
    "neoplasm": [
        ("C34.90", "Malignant neoplasm of unspecified part of right bronchus or lung"),
        ("C50.919", "Malignant neoplasm of unspecified site of unspecified female breast"),
        ("D44.0", "Neoplasm of uncertain behavior of thyroid gland"),
    ],
    "kidney": [
        ("N18.9", "Chronic kidney disease, unspecified"),
        ("N39.0", "Urinary tract infection, site not specified"),
    ],
}

# Common medications
MEDICATIONS = {
    "diabetes": [
        ("Metformin", "00093105601"),
        ("Glipizide", "00093721501"),
        ("Insulin Glargine", "00245683103"),
    ],
    "hypertension": [
        ("Lisinopril", "00093106701"),
        ("Amlodipine", "00069153066"),
        ("Hydrochlorothiazide", "00093720701"),
    ],
    "cardiovascular": [
        ("Aspirin", "00093505401"),
        ("Atorvastatin", "00071015823"),
        ("Metoprolol", "00093720801"),
    ],
    "antibiotics": [
        ("Amoxicillin", "00093419001"),
        ("Azithromycin", "00093720901"),
        ("Ciprofloxacin", "00093720201"),
    ],
    "pain": [
        ("Acetaminophen", "00093104801"),
        ("Ibuprofen", "00093505201"),
        ("Oxycodone", "00406026401"),
    ],
}

# Common lab tests
LAB_TESTS = [
    ("Glucose", "2345-7", "mg/dL", "70-100"),
    ("HbA1c", "4548-4", "%", "4.0-5.6"),
    ("Creatinine", "2160-0", "mg/dL", "0.7-1.3"),
    ("BUN", "3094-0", "mg/dL", "7-20"),
    ("Sodium", "2951-2", "mEq/L", "135-145"),
    ("Potassium", "2823-3", "mEq/L", "3.5-5.0"),
    ("Cholesterol", "2093-3", "mg/dL", "125-200"),
    ("Triglycerides", "2571-8", "mg/dL", "40-150"),
    ("HDL", "2085-9", "mg/dL", "40-60"),
    ("LDL", "2089-1", "mg/dL", "50-100"),
    ("TSH", "3016-3", "mIU/L", "0.4-4.0"),
    ("Hemoglobin", "718-7", "g/dL", "12.0-17.5"),
    ("WBC", "6690-2", "10^3/uL", "4.5-11.0"),
    ("Platelets", "777-3", "10^3/uL", "150-400"),
]

# Common procedures
PROCEDURES = [
    ("99213", "Office visit, established patient, low complexity"),
    ("99214", "Office visit, established patient, moderate complexity"),
    ("99215", "Office visit, established patient, high complexity"),
    ("99385", "Preventive visit, new patient, 18-39 years"),
    ("99386", "Preventive visit, new patient, 40-64 years"),
    ("99395", "Preventive visit, established patient, 18-39 years"),
    ("99396", "Preventive visit, established patient, 40-64 years"),
    ("80053", "Comprehensive metabolic panel"),
    ("83036", "Hemoglobin A1c"),
    ("80061", "Lipid panel"),
    ("85025", "Complete blood count with differential"),
    ("81001", "Urinalysis, with microscopy"),
    ("71046", "Chest X-ray, 2 views"),
    ("93000", "Electrocardiogram, 12-lead"),
    ("76700", "Ultrasound, abdominal, complete"),
]


class SyntheticEHRGenerator:
    """
    Generator for synthetic EHR data.
    """
    
    def __init__(self, seed: Optional[int] = None):
        """
        Initialize the generator with an optional random seed.
        
        Args:
            seed: Random seed for reproducibility
        """
        self.seed = seed
        if seed is not None:
            random.seed(seed)
    
    def _random_date(self, start_date: datetime, end_date: datetime) -> datetime:
        """Generate a random date between start_date and end_date."""
        delta = end_date - start_date
        random_days = random.randint(0, delta.days)
        return start_date + timedelta(days=random_days)
    
    def _random_zip_code(self) -> str:
        """Generate a random US zip code."""
        return f"{random.randint(10000, 99999)}"
    
    def generate_patients(self, n_patients: int) -> List[Tuple]:
        """
        Generate synthetic patient records.
        
        Args:
            n_patients: Number of patients to generate
            
        Returns:
            List of patient tuples
        """
        patients = []
        today = datetime.now()
        
        for i in range(1, n_patients + 1):
            # Generate birth date (age 18-90)
            age = random.randint(18, 90)
            birth_date = today - timedelta(days=age * 365 + random.randint(0, 364))
            
            # ~10% chance of deceased
            death_date = None
            if random.random() < 0.1 and age > 50:
                death_date = (birth_date + timedelta(days=random.randint(age * 300, age * 365))).strftime("%Y-%m-%d")
            
            # Sex distribution: 50% F, 48% M, 2% O
            sex_rand = random.random()
            if sex_rand < 0.50:
                sex = "F"
            elif sex_rand < 0.98:
                sex = "M"
            else:
                sex = "O"
            
            # Race distribution (simplified)
            race_choices = ["White", "Black", "Asian", "Hispanic", "Other"]
            race_weights = [0.60, 0.13, 0.06, 0.18, 0.03]
            race = random.choices(race_choices, weights=race_weights, k=1)[0]
            
            # Ethnicity
            ethnicity = random.choice(["Hispanic", "Non-Hispanic", "Unknown"])
            
            patients.append((
                i,
                birth_date.strftime("%Y-%m-%d"),
                death_date,
                sex,
                race,
                ethnicity,
                self._random_zip_code(),
                datetime.now().strftime("%Y-%m-%d %H:%M:%S")
            ))
        
        return patients
    
    def generate_encounters(
        self, 
        patient_ids: List[int], 
        min_encounters: int = 1, 
        max_encounters: int = 20
    ) -> List[Tuple]:
        """
        Generate synthetic encounter records.
        
        Args:
            patient_ids: List of patient IDs
            min_encounters: Minimum encounters per patient
            max_encounters: Maximum encounters per patient
            
        Returns:
            List of encounter tuples
        """
        encounters = []
        encounter_id = 1
        
        encounter_types = ["IP", "OP", "ED", "AV"]
        encounter_weights = [0.10, 0.40, 0.15, 0.35]
        departments = ["Internal Medicine", "Cardiology", "Pulmonology", 
                      "Emergency", "Family Practice", "Endocrinology"]
        facilities = ["University Hospital", "Community Medical Center", 
                     "Health Clinic", "Specialty Practice"]
        
        for patient_id in patient_ids:
            n_encounters = random.randint(min_encounters, max_encounters)
            
            for _ in range(n_encounters):
                # Random date in last 5 years
                encounter_date = self._random_date(
                    datetime.now() - timedelta(days=5*365),
                    datetime.now()
                )
                
                encounters.append((
                    encounter_id,
                    patient_id,
                    encounter_date.strftime("%Y-%m-%d"),
                    random.choices(encounter_types, weights=encounter_weights, k=1)[0],
                    random.choice(departments),
                    random.choice(facilities)
                ))
                encounter_id += 1
        
        return encounters
    
    def generate_diagnoses(
        self, 
        encounters: List[Tuple], 
        diagnoses_per_encounter: Tuple[int, int] = (1, 5)
    ) -> List[Tuple]:
        """
        Generate synthetic diagnosis records.
        
        Args:
            encounters: List of encounter tuples
            diagnoses_per_encounter: Min/max diagnoses per encounter
            
        Returns:
            List of diagnosis tuples
        """
        diagnoses = []
        diagnosis_id = 1
        
        # Flatten all ICD codes
        all_icd_codes = []
        for category_codes in ICD10_CATEGORIES.values():
            all_icd_codes.extend(category_codes)
        
        for encounter in encounters:
            encounter_id, patient_id, encounter_date, *_ = encounter
            n_diagnoses = random.randint(*diagnoses_per_encounter)
            
            # Select random diagnoses
            selected_icd = random.sample(all_icd_codes, min(n_diagnoses, len(all_icd_codes)))
            
            for seq_num, (icd_code, _) in enumerate(selected_icd, start=1):
                diagnoses.append((
                    diagnosis_id,
                    encounter_id,
                    patient_id,
                    icd_code,
                    10,  # ICD-10
                    encounter_date,  # Same as encounter date
                    seq_num
                ))
                diagnosis_id += 1
        
        return diagnoses
    
    def generate_medications(
        self, 
        patient_ids: List[int],
        encounters: List[Tuple],
        medications_per_patient: Tuple[int, int] = (1, 10)
    ) -> List[Tuple]:
        """
        Generate synthetic medication records.
        
        Args:
            patient_ids: List of patient IDs
            encounters: List of encounter tuples
            medications_per_patient: Min/max medications per patient
            
        Returns:
            List of medication tuples
        """
        medications = []
        medication_id = 1
        
        # Build encounter lookup by patient
        patient_encounters = {}
        for enc in encounters:
            pid = enc[1]
            if pid not in patient_encounters:
                patient_encounters[pid] = []
            patient_encounters[pid].append(enc)
        
        # Flatten all medications
        all_meds = []
        for category_meds in MEDICATIONS.values():
            all_meds.extend(category_meds)
        
        for patient_id in patient_ids:
            n_meds = random.randint(*medications_per_patient)
            selected_meds = random.sample(all_meds, min(n_meds, len(all_meds)))
            
            for med_name, ndc_code in selected_meds:
                # Random start date in last 3 years
                start_date = self._random_date(
                    datetime.now() - timedelta(days=3*365),
                    datetime.now()
                )
                
                # 30% chance of having end date
                end_date = None
                if random.random() < 0.3:
                    end_date = (start_date + timedelta(days=random.randint(7, 180))).strftime("%Y-%m-%d")
                
                # Find an encounter for this patient on or before start date
                encounter_id = None
                if patient_id in patient_encounters:
                    valid_encounters = [
                        e for e in patient_encounters[patient_id]
                        if e[2] <= start_date.strftime("%Y-%m-%d")
                    ]
                    if valid_encounters:
                        encounter_id = random.choice(valid_encounters)[0]
                
                medications.append((
                    medication_id,
                    patient_id,
                    encounter_id,
                    med_name,
                    ndc_code,
                    start_date.strftime("%Y-%m-%d"),
                    end_date,
                    f"{random.choice([5, 10, 25, 50, 100])}mg",
                    random.choice(["oral", "injection", "topical"])
                ))
                medication_id += 1
        
        return medications
    
    def generate_labs(
        self, 
        patient_ids: List[int],
        encounters: List[Tuple],
        labs_per_patient: Tuple[int, int] = (2, 15)
    ) -> List[Tuple]:
        """
        Generate synthetic lab result records.
        
        Args:
            patient_ids: List of patient IDs
            encounters: List of encounter tuples
            labs_per_patient: Min/max labs per patient
            
        Returns:
            List of lab tuples
        """
        labs = []
        lab_id = 1
        
        # Build encounter lookup by patient
        patient_encounters = {}
        for enc in encounters:
            pid = enc[1]
            if pid not in patient_encounters:
                patient_encounters[pid] = []
            patient_encounters[pid].append(enc)
        
        for patient_id in patient_ids:
            n_labs = random.randint(*labs_per_patient)
            selected_tests = random.sample(LAB_TESTS, min(n_labs, len(LAB_TESTS)))
            
            for lab_name, loinc_code, unit, ref_range in selected_tests:
                # Parse reference range
                ref_low, ref_high = [float(x) for x in ref_range.split("-")]
                
                # Generate result value (90% normal, 10% abnormal)
                if random.random() < 0.90:
                    # Normal value
                    result_value = round(random.uniform(ref_low, ref_high), 2)
                    abnormal_flag = "N"
                else:
                    # Abnormal value
                    if random.random() < 0.5:
                        result_value = round(random.uniform(ref_low * 0.5, ref_low), 2)
                        abnormal_flag = "L"
                    else:
                        result_value = round(random.uniform(ref_high, ref_high * 1.5), 2)
                        abnormal_flag = "H"
                
                # Random date in last 2 years
                result_date = self._random_date(
                    datetime.now() - timedelta(days=2*365),
                    datetime.now()
                )
                
                # Find an encounter for this patient on or before result date
                encounter_id = None
                if patient_id in patient_encounters:
                    valid_encounters = [
                        e for e in patient_encounters[patient_id]
                        if e[2] <= result_date.strftime("%Y-%m-%d")
                    ]
                    if valid_encounters:
                        encounter_id = random.choice(valid_encounters)[0]
                
                labs.append((
                    lab_id,
                    patient_id,
                    encounter_id,
                    lab_name,
                    loinc_code,
                    result_value,
                    unit,
                    ref_range,
                    abnormal_flag,
                    result_date.strftime("%Y-%m-%d")
                ))
                lab_id += 1
        
        return labs
    
    def generate_procedures(
        self, 
        encounters: List[Tuple],
        procedures_per_encounter: Tuple[int, int] = (0, 3)
    ) -> List[Tuple]:
        """
        Generate synthetic procedure records.
        
        Args:
            encounters: List of encounter tuples
            procedures_per_encounter: Min/max procedures per encounter
            
        Returns:
            List of procedure tuples
        """
        procedures = []
        procedure_id = 1
        
        for encounter in encounters:
            encounter_id, patient_id, encounter_date, *_ = encounter
            n_procs = random.randint(*procedures_per_encounter)
            
            if n_procs > 0:
                selected_procs = random.sample(PROCEDURES, min(n_procs, len(PROCEDURES)))
                
                for proc_code, proc_name in selected_procs:
                    procedures.append((
                        procedure_id,
                        encounter_id,
                        patient_id,
                        proc_code,
                        proc_name,
                        encounter_date,  # Same as encounter date
                        proc_code if proc_code.startswith("9") else None  # CPT code
                    ))
                    procedure_id += 1
        
        return procedures
    
    def generate_all(self, db_path: str, n_patients: int = 100) -> None:
        """
        Generate all synthetic data and populate the database.
        
        Args:
            db_path: Path to the SQLite database file
            n_patients: Number of patients to generate
        """
        from .schema import create_database
        
        # Create database schema
        create_database(db_path)
        
        # Generate data
        patients = self.generate_patients(n_patients)
        patient_ids = [p[0] for p in patients]
        
        encounters = self.generate_encounters(patient_ids)
        diagnoses = self.generate_diagnoses(encounters)
        medications = self.generate_medications(patient_ids, encounters)
        labs = self.generate_labs(patient_ids, encounters)
        procedures = self.generate_procedures(encounters)
        
        # Insert into database
        conn = sqlite3.connect(db_path)
        cursor = conn.cursor()
        
        try:
            # Insert patients
            cursor.executemany(
                """INSERT INTO patients 
                   (patient_id, birth_date, death_date, sex, race, ethnicity, address_zip, created_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)""",
                patients
            )
            
            # Insert encounters
            cursor.executemany(
                """INSERT INTO encounters 
                   (encounter_id, patient_id, encounter_date, encounter_type, department, facility)
                   VALUES (?, ?, ?, ?, ?, ?)""",
                encounters
            )
            
            # Insert diagnoses
            cursor.executemany(
                """INSERT INTO diagnoses 
                   (diagnosis_id, encounter_id, patient_id, icd_code, icd_version, diagnosis_date, sequence_number)
                   VALUES (?, ?, ?, ?, ?, ?, ?)""",
                diagnoses
            )
            
            # Insert medications
            cursor.executemany(
                """INSERT INTO medications 
                   (medication_id, patient_id, encounter_id, medication_name, ndc_code, 
                    start_date, end_date, dosage, route)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                medications
            )
            
            # Insert labs
            cursor.executemany(
                """INSERT INTO labs 
                   (lab_id, patient_id, encounter_id, lab_name, loinc_code, result_value,
                    result_unit, reference_range, abnormal_flag, result_date)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                labs
            )
            
            # Insert procedures
            cursor.executemany(
                """INSERT INTO procedures 
                   (procedure_id, encounter_id, patient_id, procedure_code, procedure_name,
                    procedure_date, cpt_code)
                   VALUES (?, ?, ?, ?, ?, ?, ?)""",
                procedures
            )
            
            conn.commit()
            
            # Print summary
            print(f"Generated database at {db_path}")
            print(f"  Patients: {len(patients)}")
            print(f"  Encounters: {len(encounters)}")
            print(f"  Diagnoses: {len(diagnoses)}")
            print(f"  Medications: {len(medications)}")
            print(f"  Labs: {len(labs)}")
            print(f"  Procedures: {len(procedures)}")
            
        except Exception as e:
            conn.rollback()
            raise e
        finally:
            conn.close()
