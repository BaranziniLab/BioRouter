"""
cli.py – Command-line interface for medmapper.

Commands:
  lookup   – Look up a code in a terminology
  map      – Crosswalk a code between terminologies
  expand   – Expand a root code to a value set
  search   – Fuzzy search over descriptions
  validate – Check if a code is valid / active
  info     – Show loaded terminology statistics
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Optional

try:
    import click
    _HAS_CLICK = True
except ImportError:
    _HAS_CLICK = False


def _build_app():
    """Build the CLI app using Click (preferred) or a minimal argparse fallback."""
    if _HAS_CLICK:
        return _build_click_cli()
    return _build_argparse_cli()


# ── Click implementation ─────────────────────────────────────────────────────

def _build_click_cli():
    import click
    from .terminology import (
        TerminologyStore, load_concepts_csv, load_concepts_json,
        load_map_csv, load_map_json,
    )
    from .hierarchy import Hierarchy
    from .mapping import CrosswalkEngine
    from .search import ConceptSearch
    from .valueset import ValueSetExpander

    @click.group()
    @click.option("--icd10-csv", "icd10_csv", type=click.Path(exists=True), default=None, help="ICD-10 CSV file")
    @click.option("--snomed-csv", "snomed_csv", type=click.Path(exists=True), default=None, help="SNOMED CT CSV file")
    @click.option("--map-csv", "map_csv", type=click.Path(exists=True), default=None, help="Cross-map CSV file")
    @click.option("--icd10-json", "icd10_json", type=click.Path(exists=True), default=None, help="ICD-10 JSON file")
    @click.option("--snomed-json", "snomed_json", type=click.Path(exists=True), default=None, help="SNOMED CT JSON file")
    @click.option("--map-json", "map_json", type=click.Path(exists=True), default=None, help="Cross-map JSON file")
    @click.pass_context
    def cli(ctx, icd10_csv, snomed_csv, map_csv, icd10_json, snomed_json, map_json):
        """medmapper – Clinical terminology crosswalk CLI."""
        ctx.ensure_object(dict)

        store = TerminologyStore()
        if icd10_csv:
            store.add_many(load_concepts_csv(icd10_csv, "ICD-10-CM"))
        if snomed_csv:
            store.add_many(load_concepts_csv(snomed_csv, "SNOMED-CT"))
        if icd10_json:
            store.add_many(load_concepts_json(icd10_json))
        if snomed_json:
            store.add_many(load_concepts_json(snomed_json))

        ctx.obj["store"] = store
        ctx.obj["hierarchy"] = Hierarchy(store)

        map_entries = []
        if map_csv:
            map_entries = load_map_csv(map_csv)
        if map_json:
            map_entries = load_map_json(map_json)
        ctx.obj["engine"] = CrosswalkEngine(store, map_entries)
        ctx.obj["searcher"] = ConceptSearch(store)
        ctx.obj["expander"] = ValueSetExpander(store, ctx.obj["hierarchy"])

    @cli.command()
    @click.argument("terminology")
    @click.argument("code")
    @click.pass_context
    def lookup(ctx, terminology, code):
        """Look up a code: medmapper lookup ICD-10-CM E11.9"""
        store = ctx.obj["store"]
        concept = store.get(terminology, code)
        if concept:
            click.echo(f"{concept.terminology}\t{concept.code}\t{concept.description}\tactive={concept.active}")
            click.echo(f"  parents: {', '.join(concept.parent_codes) if concept.parent_codes else '(root)'}")
        else:
            click.echo(f"Not found: {terminology} {code}", err=True)
            sys.exit(1)

    @cli.command()
    @click.argument("source_terminology")
    @click.argument("source_code")
    @click.option("--target", "-t", default=None, help="Target terminology (optional filter)")
    @click.pass_context
    def map(ctx, source_terminology, source_code, target):
        """Map a code: medmapper map ICD-10-CM E11.9 -t SNOMED-CT"""
        engine = ctx.obj["engine"]
        result = engine.map_code(source_terminology, source_code, target)
        if not result.mappings:
            click.echo(f"No mapping found for {source_terminology}:{source_code}", err=True)
            sys.exit(1)
        for m in result.mappings:
            click.echo(
                f"{m.target_terminology}\t{m.target_code}\t{m.target_description}"
                f"\tgroup={m.map_group}\tpriority={m.map_priority}\tcat={m.map_category}"
            )

    @cli.command()
    @click.argument("terminology")
    @click.argument("root_code")
    @click.option("--no-root", is_flag=True, help="Exclude root from expansion")
    @click.pass_context
    def expand(ctx, terminology, root_code, no_root):
        """Expand a root code to its value set: medmapper expand SNOMED-CT 73211009"""
        expander = ctx.obj["expander"]
        vs = expander.expand(terminology, root_code, include_root=not no_root)
        click.echo(f"ValueSet: {vs.root_description} ({vs.size} members)")
        for m in vs.members:
            click.echo(f"  {m.code}\t{m.description}")

    @cli.command()
    @click.argument("query")
    @click.option("--terminology", "-t", default=None, help="Restrict to a terminology")
    @click.option("--limit", "-n", default=10, help="Max results")
    @click.pass_context
    def search(ctx, query, terminology, limit):
        """Fuzzy search: medmapper search 'diabetes mellitus'"""
        searcher = ctx.obj["searcher"]
        results = searcher.search(query, terminology=terminology, limit=limit)
        if not results:
            click.echo("No matches found.")
            return
        for r in results:
            click.echo(f"  [{r.score:.0f}] {r.concept.terminology}\t{r.code}\t{r.description}")

    @cli.command()
    @click.argument("terminology")
    @click.argument("code")
    @click.pass_context
    def validate(ctx, terminology, code):
        """Check if a code is valid/active."""
        store = ctx.obj["store"]
        ok = store.is_valid(terminology, code)
        if ok:
            click.echo(f"VALID: {terminology} {code}")
        else:
            concept = store.get(terminology, code)
            if concept and not concept.active:
                click.echo(f"INACTIVE: {terminology} {code}")
            else:
                click.echo(f"NOT FOUND: {terminology} {code}")
            sys.exit(1)

    @cli.command()
    @click.pass_context
    def info(ctx):
        """Show loaded terminology statistics."""
        store = ctx.obj["store"]
        engine = ctx.obj["engine"]
        hierarchy = ctx.obj["hierarchy"]
        click.echo(f"TerminologyStore: {len(store)} concepts")
        for term in sorted(set(c.terminology for c in store.all_concepts())):
            codes = store.codes_for(term)
            click.echo(f"  {term}: {len(codes)} codes")
        click.echo(f"CrosswalkEngine: {engine.entry_count} mappings")
        click.echo(f"Hierarchy: {hierarchy}")

    return cli


# ── argparse fallback ─────────────────────────────────────────────────────────

def _build_argparse_cli():
    import argparse
    # The argparse fallback mirrors the Click CLI but is simpler.
    # For production, install click: pip install click
    parser = argparse.ArgumentParser(prog="medmapper", description="Clinical terminology crosswalk")
    sub = parser.add_subparsers(dest="command")

    # lookup
    p_lookup = sub.add_parser("lookup", help="Look up a code")
    p_lookup.add_argument("terminology")
    p_lookup.add_argument("code")

    # map
    p_map = sub.add_parser("map", help="Map a code")
    p_map.add_argument("source_terminology")
    p_map.add_argument("source_code")
    p_map.add_argument("--target", "-t", default=None)

    # expand
    p_expand = sub.add_parser("expand", help="Expand a root code")
    p_expand.add_argument("terminology")
    p_expand.add_argument("root_code")
    p_expand.add_argument("--no-root", action="store_true")

    # search
    p_search = sub.add_parser("search", help="Fuzzy search")
    p_search.add_argument("query")
    p_search.add_argument("--terminology", "-t", default=None)
    p_search.add_argument("--limit", "-n", type=int, default=10)

    # validate
    p_validate = sub.add_parser("validate", help="Validate a code")
    p_validate.add_argument("terminology")
    p_validate.add_argument("code")

    # info
    sub.add_parser("info", help="Show statistics")

    return parser


# Entry point for `python -m medmapper`
def main():
    app = _build_app()
    if _HAS_CLICK:
        app(standalone_mode=False)
    else:
        args = app.parse_args()
        print(f"[argparse fallback] command={args.command} (install click for full CLI)")
        print("  pip install click")


if __name__ == "__main__":
    main()
