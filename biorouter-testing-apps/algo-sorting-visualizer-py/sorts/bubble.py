"""
Bubble Sort implementation with animation support.

Time Complexity: O(n²) average and worst case, O(n) best case (already sorted)
Space Complexity: O(1)
Stable: Yes
"""

from typing import List, Any, Generator
from .base import InstrumentedArray, SortState, SortAction, ActionType


def bubble_sort(data: List[Any]) -> Generator[SortState, None, None]:
    """
    Sort a list using bubble sort algorithm.
    
    Yields intermediate states for visualization.
    
    Args:
        data: List of comparable elements to sort
        
    Yields:
        SortState objects representing each step of the sorting process
    """
    arr = InstrumentedArray(data, "bubble")
    n = len(arr)
    
    if n <= 1:
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.ACCESS, (0,), None),
            algorithm="bubble"
        )
        return
    
    for i in range(n):
        swapped = False
        for j in range(0, n - i - 1):
            # Yield comparison state
            yield SortState(
                array=arr.get_snapshot(),
                action=SortAction(ActionType.COMPARE, (j, j + 1)),
                algorithm="bubble"
            )
            
            if arr.compare(j, j + 1):
                # Yield swap state
                arr.swap(j, j + 1)
                swapped = True
                yield SortState(
                    array=arr.get_snapshot(),
                    action=SortAction(ActionType.SWAP, (j, j + 1)),
                    algorithm="bubble"
                )
        
        # If no swaps occurred, array is sorted
        if not swapped:
            break
