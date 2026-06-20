"""
Score registry and DSL for clinical risk scores.

Provides a decorator-based declarative system for defining risk scores.
Each score declares its input variables, computation rules, risk
categories, and clinical interpretation.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional, Sequence, Tuple, Type

from med_risk_scores.validate import VariableSpec


@dataclass(frozen=True)
class RiskCategory:
    """A risk tier with min/max score bounds, label, and interpretation."""

    min_score: float
    max_score: float
    label: str
    interpretation: str
    color: Optional[str] = None  # optional for UI use


@dataclass
class ScoreResult:
    """The result of computing a clinical risk score."""

    score_name: str
    total_score: float
    category: RiskCategory
    contributions: Dict[str, float]
    raw_inputs: Dict[str, Any]
    messages: List[str] = field(default_factory=list)

    @property
    def risk_label(self) -> str:
        return self.category.label

    @property
    def interpretation(self) -> str:
        return self.category.interpretation

    def to_dict(self) -> Dict[str, Any]:
        return {
            "score_name": self.score_name,
            "total_score": self.total_score,
            "risk_label": self.risk_label,
            "interpretation": self.interpretation,
            "contributions": self.contributions,
            "raw_inputs": self.raw_inputs,
            "messages": self.messages,
        }


@dataclass
class ScoreDefinition:
    """
    Complete definition of a clinical risk score.

    Instances are created by the ``@register_score`` decorator.
    """

    name: str
    display_name: str
    description: str
    variables: List[VariableSpec]
    compute_fn: Callable[[Dict[str, Any]], Tuple[float, Dict[str, float]]]
    categories: List[RiskCategory]
    references: List[str] = field(default_factory=list)
    version: str = "1.0"

    # ---- helpers ----

    def classify(self, total: float) -> RiskCategory:
        """Return the RiskCategory for the given total score."""
        for cat in sorted(self.categories, key=lambda c: c.min_score, reverse=True):
            if total >= cat.min_score:
                return cat
        # fallback to lowest category
        return min(self.categories, key=lambda c: c.min_score)

    @property
    def variable_specs(self) -> List[VariableSpec]:
        return list(self.variables)

    @property
    def variable_names(self) -> List[str]:
        return [v.name for v in self.variables]


# ---------------------------------------------------------------------------
# Global registry
# ---------------------------------------------------------------------------

_REGISTRY: Dict[str, ScoreDefinition] = {}


def register_score(
    name: str,
    display_name: str,
    description: str,
    variables: List[VariableSpec],
    compute_fn: Callable[[Dict[str, Any]], Tuple[float, Dict[str, float]]],
    categories: List[RiskCategory],
    references: Optional[List[str]] = None,
    version: str = "1.0",
) -> ScoreDefinition:
    """
    Register a clinical risk score definition.

    This is the low-level API; prefer the ``@register_score_decorator`` form.
    """
    if name in _REGISTRY:
        raise ValueError(f"Score '{name}' is already registered.")
    defn = ScoreDefinition(
        name=name,
        display_name=display_name,
        description=description,
        variables=variables,
        compute_fn=compute_fn,
        categories=categories,
        references=references or [],
        version=version,
    )
    _REGISTRY[name] = defn
    return defn


def get_score(name: str) -> ScoreDefinition:
    """Look up a registered score by name (case-insensitive)."""
    key = name.lower().replace("-", "_").replace(" ", "_")
    if key not in _REGISTRY:
        available = ", ".join(sorted(_REGISTRY.keys()))
        raise KeyError(f"Unknown score '{name}'. Available: {available}")
    return _REGISTRY[key]


def list_scores() -> List[str]:
    """Return sorted list of registered score names."""
    return sorted(_REGISTRY.keys())


def all_definitions() -> Dict[str, ScoreDefinition]:
    """Return a copy of the full registry."""
    return dict(_REGISTRY)


# ---------------------------------------------------------------------------
# Decorator
# ---------------------------------------------------------------------------

def score_definition(
    name: str,
    display_name: str,
    description: str,
    variables: List[VariableSpec],
    categories: List[RiskCategory],
    references: Optional[List[str]] = None,
    version: str = "1.0",
):
    """
    Class/function decorator that registers a compute function as a risk score.

    Usage::

        @score_definition(
            name="cha2ds2_vasc",
            display_name="CHA₂DS₂-VASc",
            ...
        )
        def cha2ds2_vasc(inputs):
            ...
    """

    def decorator(fn: Callable):
        register_score(
            name=name,
            display_name=display_name,
            description=description,
            variables=variables,
            compute_fn=fn,
            categories=categories,
            references=references,
            version=version,
        )
        return fn

    return decorator
