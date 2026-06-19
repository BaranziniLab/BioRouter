"""
Heap Sort implementation with animation support.

Time Complexity: O(n log n) for all cases
Space Complexity: O(1)
Stable: No
"""

from typing import List, Any, Generator
from .base import InstrumentedArray, SortState, SortAction, ActionType


def heap_sort(data: List[Any]) -> Generator[SortState, None, None]:
    """
    Sort a list using heap sort algorithm.
    
    Yields intermediate states for visualization.
    
    Args:
        data: List of comparable elements to sort
        
    Yields:
        SortState objects representing each step of the sorting process
    """
    arr = InstrumentedArray(data, "heap")
    n = len(arr)
    
    if n <= 1:
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.ACCESS, (0,), None),
            algorithm="heap"
        )
        return
    
    # Build max heap
    yield from _build_max_heap(arr, n)
    
    # Extract elements from heap one by one
    for i in range(n - 1, 0, -1):
        # Move current root to end
        arr.swap(0, i)
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.SWAP, (0, i)),
            algorithm="heap"
        )
        
        # Heapify the reduced heap
        yield from _heapify(arr, i, 0)


def _build_max_heap(arr: InstrumentedArray, n: int) -> Generator[SortState, None, None]:
    """Build a max heap from the array."""
    # Start from the last non-leaf node
    for i in range(n // 2 - 1, -1, -1):
        yield from _heapify(arr, n, i)


def _heapify(arr: InstrumentedArray, n: int, i: int) -> Generator[SortState, None, None]:
    """Heapify a subtree rooted at index i."""
    largest = i
    left = 2 * i + 1
    right = 2 * i + 2
    
    # Yield comparison with left child
    if left < n:
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.COMPARE, (largest, left)),
            algorithm="heap"
        )
        if arr.compare(left, largest):
            largest = left
    
    # Yield comparison with right child
    if right < n:
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.COMPARE, (largest, right)),
            algorithm="heap"
        )
        if arr.compare(right, largest):
            largest = right
    
    # If largest is not root, swap and continue heapifying
    if largest != i:
        arr.swap(i, largest)
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.SWAP, (i, largest)),
            algorithm="heap"
        )
        
        # Recursively heapify the affected sub-tree
        yield from _heapify(arr, n, largest)
