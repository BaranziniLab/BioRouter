"""
Quick Sort implementation with median-of-three pivot selection and animation support.

Time Complexity: O(n log n) average case, O(n²) worst case (rare with median-of-three)
Space Complexity: O(log n) average case, O(n) worst case
Stable: No
"""

from typing import List, Any, Generator
from .base import InstrumentedArray, SortState, SortAction, ActionType


def quick_sort(data: List[Any]) -> Generator[SortState, None, None]:
    """
    Sort a list using quick sort algorithm with median-of-three pivot selection.
    
    Yields intermediate states for visualization.
    
    Args:
        data: List of comparable elements to sort
        
    Yields:
        SortState objects representing each step of the sorting process
    """
    arr = InstrumentedArray(data, "quick")
    n = len(arr)
    
    if n <= 1:
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.ACCESS, (0,), None),
            algorithm="quick"
        )
        return
    
    yield from _quick_sort_recursive(arr, 0, n - 1)


def _quick_sort_recursive(arr: InstrumentedArray, low: int, high: int) -> Generator[SortState, None, None]:
    """Recursively partition and sort subarrays."""
    if low < high:
        # Partition the array
        pivot_index = yield from _partition(arr, low, high)
        
        # Recursively sort elements before and after partition
        yield from _quick_sort_recursive(arr, low, pivot_index - 1)
        yield from _quick_sort_recursive(arr, pivot_index + 1, high)


def _median_of_three(arr: InstrumentedArray, low: int, high: int) -> int:
    """Find the median of three elements (low, mid, high) and return its index."""
    mid = (low + high) // 2
    
    # Sort the three elements
    if arr.compare(low, mid):
        arr.swap(low, mid)
    if arr.compare(low, high):
        arr.swap(low, high)
    if arr.compare(mid, high):
        arr.swap(mid, high)
    
    # Return the index of the median
    return mid


def _partition(arr: InstrumentedArray, low: int, high: int) -> Generator[int, None, None]:
    """Partition the array using median-of-three pivot selection."""
    # Use median-of-three to choose pivot
    pivot_idx = _median_of_three(arr, low, high)
    
    # Move pivot to end
    arr.swap(pivot_idx, high)
    pivot = arr[high]
    
    # Yield pivot selection state
    yield SortState(
        array=arr.get_snapshot(),
        action=SortAction(ActionType.SWAP, (pivot_idx, high)),
        algorithm="quick"
    )
    
    i = low - 1
    
    for j in range(low, high):
        # Yield comparison state
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.COMPARE, (j, high)),
            algorithm="quick"
        )
        
        if not arr.compare(j, high):  # arr[j] <= pivot
            i += 1
            if i != j:
                arr.swap(i, j)
                # Yield swap state
                yield SortState(
                    array=arr.get_snapshot(),
                    action=SortAction(ActionType.SWAP, (i, j)),
                    algorithm="quick"
                )
    
    # Move pivot to its correct position
    if i + 1 != high:
        arr.swap(i + 1, high)
        # Yield final swap state
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.SWAP, (i + 1, high)),
            algorithm="quick"
        )
    
    return i + 1
