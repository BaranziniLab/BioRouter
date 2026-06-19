"""
Merge Sort implementation with animation support.

Time Complexity: O(n log n) for all cases
Space Complexity: O(n)
Stable: Yes
"""

from typing import List, Any, Generator
from .base import InstrumentedArray, SortState, SortAction, ActionType


def merge_sort(data: List[Any]) -> Generator[SortState, None, None]:
    """
    Sort a list using merge sort algorithm.
    
    Yields intermediate states for visualization.
    
    Args:
        data: List of comparable elements to sort
        
    Yields:
        SortState objects representing each step of the sorting process
    """
    arr = InstrumentedArray(data, "merge")
    n = len(arr)
    
    if n <= 1:
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.ACCESS, (0,), None),
            algorithm="merge"
        )
        return
    
    yield from _merge_sort_recursive(arr, 0, n - 1)


def _merge_sort_recursive(arr: InstrumentedArray, left: int, right: int) -> Generator[SortState, None, None]:
    """Recursively sort and merge subarrays."""
    if left < right:
        mid = (left + right) // 2
        
        # Recursively sort first and second halves
        yield from _merge_sort_recursive(arr, left, mid)
        yield from _merge_sort_recursive(arr, mid + 1, right)
        
        # Merge the sorted halves
        yield from _merge(arr, left, mid, right)


def _merge(arr: InstrumentedArray, left: int, mid: int, right: int) -> Generator[SortState, None, None]:
    """Merge two sorted subarrays."""
    # Create temporary arrays
    left_arr = [arr[i] for i in range(left, mid + 1)]
    right_arr = [arr[i] for i in range(mid + 1, right + 1)]
    
    i = j = 0
    k = left
    
    while i < len(left_arr) and j < len(right_arr):
        # Yield comparison state
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.COMPARE, (left + i, mid + 1 + j)),
            algorithm="merge"
        )
        
        if left_arr[i] <= right_arr[j]:
            arr[k] = left_arr[i]
            i += 1
        else:
            arr[k] = right_arr[j]
            j += 1
        
        # Yield overwrite state
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.OVERWRITE, (k,)),
            algorithm="merge"
        )
        k += 1
    
    # Copy remaining elements of left_arr
    while i < len(left_arr):
        arr[k] = left_arr[i]
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.OVERWRITE, (k,)),
            algorithm="merge"
        )
        i += 1
        k += 1
    
    # Copy remaining elements of right_arr
    while j < len(right_arr):
        arr[k] = right_arr[j]
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.OVERWRITE, (k,)),
            algorithm="merge"
        )
        j += 1
        k += 1
