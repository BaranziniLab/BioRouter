"""
Insertion Sort implementation with animation support.

Time Complexity: O(n²) average and worst case, O(n) best case (already sorted)
Space Complexity: O(1)
Stable: Yes
"""

from typing import List, Any, Generator
from .base import InstrumentedArray, SortState, SortAction, ActionType


def insertion_sort(data: List[Any]) -> Generator[SortState, None, None]:
    """
    Sort a list using insertion sort algorithm.
    
    Yields intermediate states for visualization.
    
    Args:
        data: List of comparable elements to sort
        
    Yields:
        SortState objects representing each step of the sorting process
    """
    arr = InstrumentedArray(data, "insertion")
    n = len(arr)
    
    if n <= 1:
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.ACCESS, (0,), None),
            algorithm="insertion"
        )
        return
    
    for i in range(1, n):
        key = arr[i]
        j = i - 1
        
        # Yield initial comparison
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.COMPARE, (j, i)),
            algorithm="insertion"
        )
        
        while j >= 0 and arr.compare(j, j + 1):
            # Yield swap state
            arr.swap(j, j + 1)
            yield SortState(
                array=arr.get_snapshot(),
                action=SortAction(ActionType.SWAP, (j, j + 1)),
                algorithm="insertion"
            )
            j -= 1
            
            if j >= 0:
                # Yield next comparison
                yield SortState(
                    array=arr.get_snapshot(),
                    action=SortAction(ActionType.COMPARE, (j, j + 1)),
                    algorithm="insertion"
                )
