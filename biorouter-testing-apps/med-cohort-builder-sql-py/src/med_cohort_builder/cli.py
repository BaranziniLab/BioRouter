"""
Command-line interface for the cohort builder.
Provides commands to generate synthetic data, build cohorts, and export results.
"""

import os
import sys
import json
import csv
import sqlite3
from typing import Optional, List
from pathlib import Path

try:
    import typer
    from typer import Typer, Argument, Option
    from rich.console import Console
    from rich.table import Table
    from rich.progress import Progress, SpinnerColumn, TextColumn
    HAS_TYPER = True
except ImportError:
    HAS_TYPER = False

from .generate import SyntheticEHRGenerator
from .builder import CohortQueryBuilder, SQLCompiler
from .summary import CohortSummarizer
from .criteria import (
    AgeCriterion, SexCriterion, DiagnosisCriterion, 
    MedicationCriterion, LabCriterion, CohortDefinition
)


if HAS_TYPER:
    app = Typer(
        name="cohort-builder",
        help="Build patient cohorts from synthetic EHR data",
        no_args_is_help=True
    )
    console = Console()
else:
    # Fallback for when typer is not installed
    app = None
    console = None


def print_error(message: str) -> None:
    """Print error message."""
    if console:
        console.print(f"[bold red]Error:[/bold red] {message}")
    else:
        print(f"Error: {message}", file=sys.stderr)


def print_success(message: str) -> None:
    """Print success message."""
    if console:
        console.print(f"[bold green]Success:[/bold green] {message}")
    else:
        print(f"Success: {message}")


if HAS_TYPER:
    @app.command()
    def generate(
        db_path: str = Argument(
            ...,
            help="Path to the SQLite database file"
        ),
        n_patients: int = Option(
            100,
            "--patients",
            "-p",
            help="Number of patients to generate"
        ),
        seed: Optional[int] = Option(
            None,
            "--seed",
            "-s",
            help="Random seed for reproducibility"
        ),
        force: bool = Option(
            False,
            "--force",
            "-f",
            help="Overwrite existing database"
        )
    ):
        """Generate synthetic EHR data."""
        # Check if database exists
        if os.path.exists(db_path) and not force:
            print_error(f"Database already exists: {db_path}. Use --force to overwrite.")
            raise typer.Exit(1)
        
        # Remove existing database if force
        if os.path.exists(db_path) and force:
            os.remove(db_path)
        
        try:
            with Progress(
                SpinnerColumn(),
                TextColumn("[progress.description]{task.description}"),
                console=console
            ) as progress:
                task = progress.add_task("Generating synthetic data...", total=None)
                
                generator = SyntheticEHRGenerator(seed=seed)
                generator.generate_all(db_path, n_patients)
                
                progress.update(task, description="Complete!")
            
            print_success(f"Generated database at {db_path}")
            
        except Exception as e:
            print_error(f"Failed to generate data: {e}")
            raise typer.Exit(1)


    @app.command()
    def build(
        db_path: str = Argument(
            ...,
            help="Path to the SQLite database"
        ),
        definition_file: str = Argument(
            ...,
            help="Path to JSON cohort definition file"
        ),
        output_csv: Optional[str] = Option(
            None,
            "--output",
            "-o",
            help="Output CSV file path"
        ),
        show_summary: bool = Option(
            True,
            "--summary/--no-summary",
            help="Show cohort summary statistics"
        )
    ):
        """Build a cohort from a JSON definition file."""
        # Load definition
        try:
            with open(definition_file, 'r') as f:
                definition_data = json.load(f)
            
            definition = CohortDefinition.from_dict(definition_data)
            
        except FileNotFoundError:
            print_error(f"Definition file not found: {definition_file}")
            raise typer.Exit(1)
        except json.JSONDecodeError as e:
            print_error(f"Invalid JSON: {e}")
            raise typer.Exit(1)
        
        # Build and execute query
        try:
            builder = CohortQueryBuilder(db_path)
            builder.definition = definition
            
            query = builder.build()
            
            if console:
                console.print("\n[bold]SQL Query:[/bold]")
                console.print(query.sql)
                console.print(f"\n[bold]Parameters:[/bold] {query.params}")
            
            # Execute query
            patient_ids = builder.execute()
            
            print_success(f"Cohort '{definition.name}' built: {len(patient_ids)} patients")
            
            # Export to CSV if requested
            if output_csv:
                export_patients_to_csv(db_path, patient_ids, output_csv)
                print_success(f"Exported to {output_csv}")
            
            # Show summary if requested
            if show_summary:
                summarizer = CohortSummarizer(db_path)
                summary = summarizer.summarize(patient_ids, definition.name)
                summary.print_summary()
            
        except Exception as e:
            print_error(f"Failed to build cohort: {e}")
            raise typer.Exit(1)


    @app.command()
    def query(
        db_path: str = Argument(
            ...,
            help="Path to the SQLite database"
        ),
        sql: str = Argument(
            ...,
            help="SQL query to execute"
        ),
        output_csv: Optional[str] = Option(
            None,
            "--output",
            "-o",
            help="Output CSV file path"
        )
    ):
        """Execute a custom SQL query."""
        try:
            conn = sqlite3.connect(db_path)
            cursor = conn.cursor()
            
            cursor.execute(sql)
            
            # Get column names
            columns = [description[0] for description in cursor.description] if cursor.description else []
            
            # Get results
            results = cursor.fetchall()
            
            conn.close()
            
            if not results:
                console.print("[yellow]No results returned[/yellow]")
                return
            
            # Print results
            if console:
                table = Table(title="Query Results")
                for col in columns:
                    table.add_column(col)
                
                for row in results[:100]:  # Limit to 100 rows
                    table.add_row(*[str(val) for val in row])
                
                console.print(table)
                
                if len(results) > 100:
                    console.print(f"\n[yellow]Showing first 100 of {len(results)} results[/yellow]")
            
            # Export if requested
            if output_csv:
                with open(output_csv, 'w', newline='') as f:
                    writer = csv.writer(f)
                    writer.writerow(columns)
                    writer.writerows(results)
                print_success(f"Exported to {output_csv}")
            
        except Exception as e:
            print_error(f"Query failed: {e}")
            raise typer.Exit(1)


    @app.command()
    def export(
        db_path: str = Argument(
            ...,
            help="Path to the SQLite database"
        ),
        output_csv: str = Argument(
            ...,
            help="Output CSV file path"
        ),
        patient_ids: Optional[str] = Option(
            None,
            "--patients",
            help="Comma-separated patient IDs (export all if not specified)"
        )
    ):
        """Export patient data to CSV."""
        try:
            conn = sqlite3.connect(db_path)
            cursor = conn.cursor()
            
            if patient_ids:
                ids = [int(id.strip()) for id in patient_ids.split(",")]
                placeholders = ", ".join(["?" for _ in ids])
                
                # Get patient data
                cursor.execute(f"""
                    SELECT * FROM patients 
                    WHERE patient_id IN ({placeholders})
                """, ids)
            else:
                cursor.execute("SELECT * FROM patients")
            
            # Get column names
            columns = [description[0] for description in cursor.description]
            results = cursor.fetchall()
            
            conn.close()
            
            # Write CSV
            with open(output_csv, 'w', newline='') as f:
                writer = csv.writer(f)
                writer.writerow(columns)
                writer.writerows(results)
            
            print_success(f"Exported {len(results)} patients to {output_csv}")
            
        except Exception as e:
            print_error(f"Export failed: {e}")
            raise typer.Exit(1)


    def export_patients_to_csv(db_path: str, patient_ids: List[int], output_path: str) -> None:
        """Export specific patients to CSV."""
        conn = sqlite3.connect(db_path)
        cursor = conn.cursor()
        
        placeholders = ", ".join(["?" for _ in patient_ids])
        
        cursor.execute(f"""
            SELECT * FROM patients 
            WHERE patient_id IN ({placeholders})
            ORDER BY patient_id
        """, patient_ids)
        
        columns = [description[0] for description in cursor.description]
        results = cursor.fetchall()
        
        conn.close()
        
        with open(output_path, 'w', newline='') as f:
            writer = csv.writer(f)
            writer.writerow(columns)
            writer.writerows(results)


def main():
    """Main entry point."""
    if app:
        app()
    else:
        print("Error: typer is not installed. Install with: pip install typer[all]")
        print("\nAvailable commands:")
        print("  generate  - Generate synthetic EHR data")
        print("  build     - Build a cohort from JSON definition")
        print("  query     - Execute custom SQL query")
        print("  export    - Export patient data to CSV")
        sys.exit(1)


if __name__ == "__main__":
    main()
