"""
Trial design module.

Provides fixed, group-sequential, and response-adaptive designs.
"""

from .fixed import FixedDesign
from .group_sequential import GroupSequentialDesign
from .response_adaptive import ResponseAdaptiveDesign

__all__ = ["FixedDesign", "GroupSequentialDesign", "ResponseAdaptiveDesign"]
