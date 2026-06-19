"""
Counting Sort implementation with animation support.

Time Complexity: O(n + k) where k is the range of input
Space Complexity: O(n + k)
Stable: Yes (when implemented correctly)
"""

from typing import List, Any, Generator
from .base import InstrumentedArray, SortState, SortAction, ActionType


def counting_sort(data: List[Any]) -> Generator[SortState, None, None]:
    """
    Sort a list using counting sort algorithm.
    
    Assumes input consists of non-negative integers.
    Yields intermediate states for visualization.
    
    Args:
        data: List of non-negative integers to sort
        
    Yields:
        SortState objects representing each step of the sorting process
    """
    if not data:
        yield SortState(
            array=[],
            action=SortAction(ActionType.ACCESS, (0,), None),
            algorithm="counting"
        )
        return
    
    # Find the maximum value to determine range
    max_val = max(data)
    min_val = min(data)
    range_val = max_val - min_val + 1
    
    # Create count array
    count = [0] * range_val
    output = [0] * len(data)
    
    # Store count of each character
    arr = InstrumentedArray(data, "counting")
    for i in range(len(arr)):
        val = arr[i]
        count[val - min_val] += 1
        # Yield access state
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.ACCESS, (i,)),
            algorithm="counting"
        )
    
    # Change count[i] so that count[i] now contains actual
    # position of this character in output array
    for i in range(1, len(count)):
        count[i] += count[i - 1]
    
    # Build the output character array
    # To make it stable, we work backwards
    for i in range(len(arr) - 1, -1, -1):
        val = arr[i]
        output[count[val - min_val] - 1] = val
        count[val - min_val] -= 1
        
        # Yield overwrite state
        yield SortState(
            array=output.copy(),
            action=SortAction(ActionType.OVERWRITE, (count[val - min_val],)),
            algorithm="counting"
        )
    
    # Copy the output array to arr
    for i in range(len(arr)):
        arr[i] = output[i]
        # Yield final overwrite state
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.OVERWRITE, (i,)),
            algorithm="counting"
        )
