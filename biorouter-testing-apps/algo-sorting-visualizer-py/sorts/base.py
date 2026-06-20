"""
Base class for sorting algorithms with instrumentation.

Provides common functionality for counting comparisons, swaps, and array accesses.
"""

from typing import List, Any, Generator, Tuple, Optional
from dataclasses import dataclass
from enum import Enum


class ActionType(Enum):
    """Types of actions that can be performed during sorting."""
    COMPARE = "compare"
    SWAP = "swap"
    ACCESS = "access"
    OVERWRITE = "overwrite"


@dataclass
class SortAction:
    """Represents an action performed during sorting."""
    action_type: ActionType
    indices: Tuple[int, ...]
    values: Optional[Tuple[Any, ...]] = None


@dataclass
class SortState:
    """Represents a snapshot of the array during sorting."""
    array: List[Any]
    action: SortAction
    algorithm: str


class InstrumentedArray:
    """Wrapper around a list that tracks comparisons, swaps, and accesses."""
    
    def __init__(self, data: List[Any], algorithm: str = "unknown"):
        self._data = data.copy()
        self.algorithm = algorithm
        self.comparisons = 0
        self.swaps = 0
        self.accesses = 0
        self.overwrites = 0
    
    def __len__(self) -> int:
        return len(self._data)
    
    def __getitem__(self, index: int) -> Any:
        self.accesses += 1
        return self._data[index]
    
    def __setitem__(self, index: int, value: Any):
        self._data[index] = value
        self.overwrites += 1
    
    def __iter__(self):
        return iter(self._data)
    
    def __repr__(self) -> str:
        return f"InstrumentedArray({self._data})"
    
    def compare(self, i: int, j: int) -> bool:
        """Compare elements at indices i and j. Returns True if arr[i] > arr[j]."""
        self.comparisons += 1
        self.accesses += 2
        return self._data[i] > self._data[j]
    
    def swap(self, i: int, j: int):
        """Swap elements at indices i and j."""
        self.swaps += 1
        self.accesses += 4
        self._data[i], self._data[j] = self._data[j], self._data[i]
    
    def get_snapshot(self) -> List[Any]:
        """Return a copy of the current array state."""
        return self._data.copy()
    
    def get_stats(self) -> dict:
        """Return current statistics."""
        return {
            'comparisons': self.comparisons,
            'swaps': self.swaps,
            'accesses': self.accesses,
            'overwrites': self.overwrites
        }


def yield_state(arr: InstrumentedArray, action: SortAction) -> SortState:
    """Create a SortState from the current array and action."""
    return SortState(
        array=arr.get_snapshot(),
        action=action,
        algorithm=arr.algorithm
    )
