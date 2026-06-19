"""
Radix Sort implementation with animation support.

Time Complexity: O(d * (n + k)) where d is the number of digits and k is the range of each digit
Space Complexity: O(n + k)
Stable: Yes
"""

from typing import List, Any, Generator
from .base import InstrumentedArray, SortState, SortAction, ActionType


def radix_sort(data: List[Any]) -> Generator[SortState, None, None]:
    """
    Sort a list using radix sort algorithm (LSD - Least Significant Digit).
    
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
            algorithm="radix"
        )
        return
    
    arr = InstrumentedArray(data, "radix")
    
    # Find the maximum number to know number of digits
    max_val = max(arr)
    
    # Do counting sort for every digit
    exp = 1
    while max_val // exp > 0:
        yield from _counting_sort_by_digit(arr, exp)
        exp *= 10


def _counting_sort_by_digit(arr: InstrumentedArray, exp: int) -> Generator[SortState, None, None]:
    """Perform counting sort on the array based on the digit at position exp."""
    n = len(arr)
    output = [0] * n
    count = [0] * 10  # 10 digits (0-9)
    
    # Store count of occurrences in count[]
    for i in range(n):
        digit = (arr[i] // exp) % 10
        count[digit] += 1
        # Yield access state
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.ACCESS, (i,)),
            algorithm="radix"
        )
    
    # Change count[i] so that count[i] now contains actual
    # position of this digit in output[]
    for i in range(1, 10):
        count[i] += count[i - 1]
    
    # Build the output array
    # To make it stable, we work backwards
    for i in range(n - 1, -1, -1):
        digit = (arr[i] // exp) % 10
        output[count[digit] - 1] = arr[i]
        count[digit] -= 1
        
        # Yield overwrite state
        yield SortState(
            array=output.copy(),
            action=SortAction(ActionType.OVERWRITE, (count[digit],)),
            algorithm="radix"
        )
    
    # Copy the output array to arr
    for i in range(n):
        arr[i] = output[i]
        # Yield final overwrite state
        yield SortState(
            array=arr.get_snapshot(),
            action=SortAction(ActionType.OVERWRITE, (i,)),
            algorithm="radix"
        )
