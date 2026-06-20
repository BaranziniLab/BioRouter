"""
Benchmark harness for sorting algorithms.

Provides functionality to compare algorithms across different input sizes and distributions.
"""

import time
import random
from typing import List, Any, Callable, Generator, Dict, Tuple, Optional
from dataclasses import dataclass
from .base import SortState
from .instrument import SortStats, estimate_stats


@dataclass
class BenchmarkResult:
    """Result of a benchmark run."""
    algorithm: str
    size: int
    distribution: str
    time_taken: float
    comparisons: int
    swaps: int
    accesses: int
    overwrites: int


def generate_random_array(size: int, max_value: int = None) -> List[int]:
    """
    Generate a random array of integers.
    
    Args:
        size: Size of the array
        max_value: Maximum value (default: size * 2)
        
    Returns:
        List of random integers
    """
    if max_value is None:
        max_value = size * 2
    return [random.randint(0, max_value) for _ in range(size)]


def generate_sorted_array(size: int) -> List[int]:
    """
    Generate a sorted array of integers.
    
    Args:
        size: Size of the array
        
    Returns:
        Sorted list of integers
    """
    return list(range(size))


def generate_reversed_array(size: int) -> List[int]:
    """
    Generate a reversed array of integers.
    
    Args:
        size: Size of the array
        
    Returns:
        Reversed list of integers
    """
    return list(range(size, 0, -1))


def generate_few_unique_array(size: int, num_unique: int = 10) -> List[int]:
    """
    Generate an array with few unique values.
    
    Args:
        size: Size of the array
        num_unique: Number of unique values
        
    Returns:
        List with few unique values
    """
    return [random.randint(0, num_unique - 1) for _ in range(size)]


def get_distribution_generator(distribution: str, size: int) -> List[int]:
    """
    Get an array generator based on distribution type.
    
    Args:
        distribution: Type of distribution ('random', 'sorted', 'reversed', 'few-unique')
        size: Size of the array
        
    Returns:
        Generated array
    """
    if distribution == 'random':
        return generate_random_array(size)
    elif distribution == 'sorted':
        return generate_sorted_array(size)
    elif distribution == 'reversed':
        return generate_reversed_array(size)
    elif distribution == 'few-unique':
        return generate_few_unique_array(size)
    else:
        raise ValueError(f"Unknown distribution: {distribution}")


def benchmark_algorithm(sort_func: Callable[[List[Any]], Generator[SortState, None, None]], 
                       data: List[Any]) -> Tuple[float, SortStats]:
    """
    Benchmark a sorting algorithm.
    
    Args:
        sort_func: Sorting function that yields SortState objects
        data: List of elements to sort
        
    Returns:
        Tuple of (time_taken, statistics)
    """
    arr = data.copy()
    algorithm = sort_func.__name__.replace('_sort', '')
    
    # Time the sorting
    start_time = time.perf_counter()
    
    # Run the sorting algorithm and consume all states
    last_state = None
    for state in sort_func(arr):
        last_state = state
    
    end_time = time.perf_counter()
    time_taken = end_time - start_time
    
    # Get statistics
    stats = estimate_stats(algorithm, len(data))
    
    return time_taken, stats


def run_benchmark(algorithms: Dict[str, Callable], 
                 sizes: List[int],
                 distributions: List[str],
                 num_trials: int = 3,
                 seed: Optional[int] = None) -> List[BenchmarkResult]:
    """
    Run benchmarks for multiple algorithms across different sizes and distributions.
    
    Args:
        algorithms: Dictionary of algorithm_name -> sort_function
        sizes: List of array sizes to test
        distributions: List of distribution types to test
        num_trials: Number of trials for each combination
        seed: Random seed for reproducible data generation
        
    Returns:
        List of BenchmarkResult objects
    """
    results = []
    
    for size in sizes:
        for distribution in distributions:
            print(f"\nBenchmarking size={size}, distribution={distribution}")
            
            for algo_name, sort_func in algorithms.items():
                print(f"  Running {algo_name}...", end=" ", flush=True)
                
                trial_times = []
                trial_stats = None
                
                for trial in range(num_trials):
                    # Generate data with seed offset for each trial
                    trial_seed = (seed + trial) if seed is not None else None
                    if trial_seed is not None:
                        random.seed(trial_seed)
                    
                    data = get_distribution_generator(distribution, size)
                    
                    # Run benchmark
                    time_taken, stats = benchmark_algorithm(sort_func, data)
                    trial_times.append(time_taken)
                    trial_stats = stats
                
                # Calculate average time
                avg_time = sum(trial_times) / len(trial_times)
                
                # Create result
                result = BenchmarkResult(
                    algorithm=algo_name,
                    size=size,
                    distribution=distribution,
                    time_taken=avg_time,
                    comparisons=trial_stats.comparisons,
                    swaps=trial_stats.swaps,
                    accesses=trial_stats.accesses,
                    overwrites=trial_stats.overwrites
                )
                
                results.append(result)
                print(f"{avg_time:.4f}s")
    
    return results


def format_benchmark_table(results: List[BenchmarkResult]) -> str:
    """
    Format benchmark results as a table.
    
    Args:
        results: List of BenchmarkResult objects
        
    Returns:
        Formatted table string
    """
    if not results:
        return "No results to display."
    
    # Group results by size and distribution
    grouped = {}
    for result in results:
        key = (result.size, result.distribution)
        if key not in grouped:
            grouped[key] = []
        grouped[key].append(result)
    
    # Create table
    lines = []
    lines.append("=" * 80)
    lines.append("BENCHMARK RESULTS")
    lines.append("=" * 80)
    
    for (size, distribution), group in grouped.items():
        lines.append(f"\nSize: {size}, Distribution: {distribution}")
        lines.append("-" * 60)
        lines.append(f"{'Algorithm':<15} {'Time (s)':<12} {'Comparisons':<12} {'Swaps':<12}")
        lines.append("-" * 60)
        
        # Sort by time
        group.sort(key=lambda x: x.time_taken)
        
        for result in group:
            lines.append(
                f"{result.algorithm:<15} "
                f"{result.time_taken:<12.4f} "
                f"{result.comparisons:<12} "
                f"{result.swaps:<12}"
            )
    
    lines.append("\n" + "=" * 80)
    
    return "\n".join(lines)


def get_fastest_algorithm(results: List[BenchmarkResult], 
                         size: int, 
                         distribution: str) -> str:
    """
    Get the fastest algorithm for a given size and distribution.
    
    Args:
        results: List of BenchmarkResult objects
        size: Array size
        distribution: Distribution type
        
    Returns:
        Name of the fastest algorithm
    """
    filtered = [r for r in results if r.size == size and r.distribution == distribution]
    
    if not filtered:
        return "Unknown"
    
    fastest = min(filtered, key=lambda x: x.time_taken)
    return fastest.algorithm
