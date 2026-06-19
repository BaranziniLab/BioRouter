"""
Shell Sort implementation with animation support.

Time Complexity: O(n log²n) average case, depends on gap sequence
Space Complexity: O(1)
Stable: No
"""

from typing import List, Any, Generator
from .base import InstrumentedArray, SortState, SortAction, ActionType


def shell_sort(data: List[Any]) -> Generator[SortState, None, None]:
    """
    Sort a list using shell sort algorithm with Ciura's gap sequence.
    
    Yields intermediate states for visualization.
    
    Args:
        data: List of comparable elements to sort
        
    Yields:
        SortState objects representing each step of the sorting process
    """
    arr = InstrumentedArray(data, "shell")
    n = len(arr)
    
    if n <= 1:
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.ACCESS, (0,), None),
            algorithm="shell"
        )
        return
    
    # Ciura's gap sequence (empirically good)
    gaps = [701, 301, 132, 57, 23, 10, 4, 1]
    
    # Find the appropriate starting gap
    gap = 1
    for g in gaps:
        if g < n:
            gap = g
            break
    
    while gap > 0:
        # Do a gapped insertion sort
        for i in range(gap, n):
            temp = arr[i]
            j = i
            
            # Yield comparison state
            if j >= gap:
                yield SortState(
                    array=arr.get_snapshot(),
                    action=SortAction(ActionType.COMPARE, (j - gap, j)),
                    algorithm="shell"
                )
            
            while j >= gap and arr.compare(j - gap, j):
                # Yield swap state
                arr.swap(j - gap, j)
                yield SortState(
                    array=arr.get_snapshot(),
                    action=SortAction(ActionType.SWAP, (j - gap, j)),
                    algorithm="shell"
                )
                j -= gap
                
                if j >= gap:
                    # Yield next comparison
                    yield SortState(
                        array=arr.get_snapshot(),
                        action=SortAction(ActionType.COMPARE, (j - gap, j)),
                        algorithm="shell"
                    )
        
        # Move to the next gap
        gap = next((g for g in gaps if g < gap), 0)
