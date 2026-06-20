"""
Sorting Algorithm Library with Animation Support

Each sorting algorithm is implemented as a generator that yields intermediate states
for visualization. The generator yields tuples of (array_snapshot, indices_being_compared_or_swapped).
"""

from .bubble import bubble_sort
from .insertion import insertion_sort
from .selection import selection_sort
from .merge import merge_sort
from .quick import quick_sort
from .heap import heap_sort
from .shell import shell_sort
from .counting import counting_sort
from .radix import radix_sort

__all__ = [
    'bubble_sort',
    'insertion_sort',
    'selection_sort',
    'merge_sort',
    'quick_sort',
    'heap_sort',
    'shell_sort',
    'counting_sort',
    'radix_sort'
]
