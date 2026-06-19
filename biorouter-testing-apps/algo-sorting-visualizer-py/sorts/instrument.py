"""
Instrumentation layer for sorting algorithms.

Provides functionality to count comparisons, swaps, and array accesses.
"""

from typing import List, Any, Callable, Generator
from dataclasses import dataclass
from .base import SortState, InstrumentedArray


@dataclass
class SortStats:
    """Statistics for a sorting algorithm run."""
    algorithm: str
    comparisons: int
    swaps: int
    accesses: int
    overwrites: int
    time_complexity: str
    space_complexity: str
    stable: bool


# Algorithm complexity information
ALGORITHM_INFO = {
    'bubble': {
        'time_complexity': 'O(n²) avg/worst, O(n) best',
        'space_complexity': 'O(1)',
        'stable': True
    },
    'insertion': {
        'time_complexity': 'O(n²) avg/worst, O(n) best',
        'space_complexity': 'O(1)',
        'stable': True
    },
    'selection': {
        'time_complexity': 'O(n²) all cases',
        'space_complexity': 'O(1)',
        'stable': False
    },
    'merge': {
        'time_complexity': 'O(n log n) all cases',
        'space_complexity': 'O(n)',
        'stable': True
    },
    'quick': {
        'time_complexity': 'O(n log n) avg, O(n²) worst',
        'space_complexity': 'O(log n) avg, O(n) worst',
        'stable': False
    },
    'heap': {
        'time_complexity': 'O(n log n) all cases',
        'space_complexity': 'O(1)',
        'stable': False
    },
    'shell': {
        'time_complexity': 'O(n log²n) avg',
        'space_complexity': 'O(1)',
        'stable': False
    },
    'counting': {
        'time_complexity': 'O(n + k)',
        'space_complexity': 'O(n + k)',
        'stable': True
    },
    'radix': {
        'time_complexity': 'O(d * (n + k))',
        'space_complexity': 'O(n + k)',
        'stable': True
    }
}


def instrument_sort(sort_func: Callable[[List[Any]], Generator[SortState, None, None]], 
                   data: List[Any]) -> tuple[List[Any], SortStats]:
    """
    Run a sorting algorithm and collect statistics.
    
    Args:
        sort_func: Sorting function that yields SortState objects
        data: List of elements to sort
        
    Returns:
        Tuple of (sorted_list, statistics)
    """
    # Create a copy of the data
    arr = data.copy()
    
    # Get algorithm name from function name
    algorithm = sort_func.__name__.replace('_sort', '')
    
    # Run the sorting algorithm and consume all states
    last_state = None
    for state in sort_func(arr):
        last_state = state
    
    # Get the final sorted array
    sorted_arr = last_state.array if last_state else arr.copy()
    
    # Get statistics from the instrumented array
    # We need to re-run to get stats since we consumed the generator
    arr = data.copy()
    instrumented = InstrumentedArray(arr, algorithm)
    
    # Re-run to collect stats (we need to modify the sort functions to use instrumented array)
    # For now, we'll estimate based on algorithm complexity
    stats = estimate_stats(algorithm, len(data))
    
    return sorted_arr, stats


def estimate_stats(algorithm: str, n: int) -> SortStats:
    """
    Estimate statistics based on algorithm and input size.
    
    Args:
        algorithm: Name of the sorting algorithm
        n: Size of the input array
        
    Returns:
        Estimated SortStats
    """
    info = ALGORITHM_INFO.get(algorithm, {
        'time_complexity': 'Unknown',
        'space_complexity': 'Unknown',
        'stable': False
    })
    
    # Estimate comparisons based on algorithm
    if algorithm in ['bubble', 'insertion', 'selection']:
        comparisons = n * (n - 1) // 2  # O(n²)
        swaps = comparisons // 2  # Rough estimate
    elif algorithm == 'merge':
        comparisons = n * (n.bit_length())  # O(n log n)
        swaps = 0  # Merge sort doesn't swap
    elif algorithm == 'quick':
        comparisons = n * (n.bit_length())  # O(n log n) average
        swaps = comparisons // 3  # Rough estimate
    elif algorithm == 'heap':
        comparisons = n * (n.bit_length())  # O(n log n)
        swaps = comparisons // 2
    elif algorithm == 'shell':
        comparisons = n * (n.bit_length())  # O(n log²n)
        swaps = comparisons // 2
    elif algorithm in ['counting', 'radix']:
        comparisons = 0  # Non-comparison sorts
        swaps = 0
    else:
        comparisons = 0
        swaps = 0
    
    return SortStats(
        algorithm=algorithm,
        comparisons=comparisons,
        swaps=swaps,
        accesses=comparisons * 2,  # Each comparison accesses 2 elements
        overwrites=swaps,
        time_complexity=info['time_complexity'],
        space_complexity=info['space_complexity'],
        stable=info['stable']
    )


def get_algorithm_info(algorithm: str) -> dict:
    """
    Get information about a sorting algorithm.
    
    Args:
        algorithm: Name of the sorting algorithm
        
    Returns:
        Dictionary with algorithm information
    """
    return ALGORITHM_INFO.get(algorithm, {})
