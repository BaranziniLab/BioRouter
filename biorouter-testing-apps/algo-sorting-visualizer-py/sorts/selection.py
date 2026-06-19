"""
Selection Sort implementation with animation support.

Time Complexity: O(n²) for all cases
Space Complexity: O(1)
Stable: No
"""

from typing import List, Any, Generator
from .base import InstrumentedArray, SortState, SortAction, ActionType


def selection_sort(data: List[Any]) -> Generator[SortState, None, None]:
    """
    Sort a list using selection sort algorithm.
    
    Yields intermediate states for visualization.
    
    Args:
        data: List of comparable elements to sort
        
    Yields:
        SortState objects representing each step of the sorting process
    """
    arr = InstrumentedArray(data, "selection")
    n = len(arr)
    
    if n <= 1:
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.ACCESS, (0,), None),
            algorithm="selection"
        )
        return
    
    for i in range(n):
        min_idx = i
        
        for j in range(i + 1, n):
            # Yield comparison state
            yield SortState(
                array=arr.get_snapshot(),
                action=SortAction(ActionType.COMPARE, (min_idx, j)),
                algorithm="selection"
            )
            
            if arr.compare(min_idx, j):
                min_idx = j
        
        if min_idx != i:
            # Yield swap state
            arr.swap(i, min_idx)
            yield SortState(
                array=arr.get_snapshot(),
                action=SortAction(ActionType.SWAP, (i, min_idx)),
                algorithm="selection"
            )
