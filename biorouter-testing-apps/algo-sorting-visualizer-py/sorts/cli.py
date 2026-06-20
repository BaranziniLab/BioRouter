"""
Command Line Interface for the sorting algorithm visualizer.

Uses argparse subcommands for clean separation of functionality:
- sort: Animate a chosen algorithm on a seeded array
- bench: Run the benchmark table
- list: List available algorithms or distributions
"""

import argparse
import sys
import random
from typing import List, Any, Optional

from . import bubble_sort, insertion_sort, selection_sort, merge_sort, quick_sort
from . import heap_sort, shell_sort, counting_sort, radix_sort
from .viz import visualize_sorting
from .bench import (
    run_benchmark, format_benchmark_table, get_distribution_generator,
    generate_random_array, generate_sorted_array, generate_reversed_array,
    generate_few_unique_array
)
from .instrument import ALGORITHM_INFO


# Available algorithms
ALGORITHMS = {
    'bubble': bubble_sort,
    'insertion': insertion_sort,
    'selection': selection_sort,
    'merge': merge_sort,
    'quick': quick_sort,
    'heap': heap_sort,
    'shell': shell_sort,
    'counting': counting_sort,
    'radix': radix_sort
}

# Available distributions
DISTRIBUTIONS = ['random', 'sorted', 'reversed', 'few-unique']


def validate_algorithm(name: str) -> str:
    """Validate and return algorithm name, raising error if unknown."""
    if name not in ALGORITHMS:
        raise argparse.ArgumentTypeError(
            f"unknown algorithm '{name}'. "
            f"Available algorithms: {', '.join(sorted(ALGORITHMS.keys()))}"
        )
    return name


def validate_distribution(name: str) -> str:
    """Validate and return distribution name, raising error if unknown."""
    if name not in DISTRIBUTIONS:
        raise argparse.ArgumentTypeError(
            f"unknown distribution '{name}'. "
            f"Available distributions: {', '.join(DISTRIBUTIONS)}"
        )
    return name


def validate_positive_int(value: str) -> int:
    """Validate and return a positive integer."""
    try:
        ivalue = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError(f"invalid int value: '{value}'")
    if ivalue <= 0:
        raise argparse.ArgumentTypeError(f"value must be positive, got {ivalue}")
    return ivalue


def validate_positive_float(value: str) -> float:
    """Validate and return a positive float."""
    try:
        fvalue = float(value)
    except ValueError:
        raise argparse.ArgumentTypeError(f"invalid float value: '{value}'")
    if fvalue < 0:
        raise argparse.ArgumentTypeError(f"value must be non-negative, got {fvalue}")
    return fvalue


def generate_array(distribution: str, size: int, seed: Optional[int] = None) -> List[int]:
    """
    Generate an array based on distribution type and optional seed.
    
    Args:
        distribution: Type of distribution
        size: Size of the array
        seed: Random seed for reproducibility
        
    Returns:
        Generated array
    """
    if seed is not None:
        random.seed(seed)
    
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


def build_parser() -> argparse.ArgumentParser:
    """Build the argument parser with subcommands."""
    parser = argparse.ArgumentParser(
        prog='sorting-viz',
        description='Sorting Algorithm Visualizer and Benchmark Tool',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  sorting-viz sort bubble -n 20
  sorting-viz sort quick -n 30 -d sorted --seed 42
  sorting-viz bench --sizes 100 500 1000
  sorting-viz bench -a bubble quick heap --distributions random sorted
  sorting-viz list
  sorting-viz list distributions
        """
    )
    
    subparsers = parser.add_subparsers(dest='command', help='Available commands')
    
    # ---- sort subcommand ----
    sort_parser = subparsers.add_parser(
        'sort',
        help='Animate a sorting algorithm on an array',
        description='Visualize a sorting algorithm with animated terminal output'
    )
    sort_parser.add_argument(
        'algorithm',
        type=validate_algorithm,
        help='Sorting algorithm to visualize'
    )
    sort_parser.add_argument(
        '-n', '--size',
        type=validate_positive_int,
        default=20,
        help='Size of the array to sort (default: 20)'
    )
    sort_parser.add_argument(
        '-d', '--distribution',
        type=validate_distribution,
        default='random',
        help='Distribution of the array (default: random)'
    )
    sort_parser.add_argument(
        '-s', '--speed',
        type=validate_positive_float,
        default=0.1,
        help='Speed of animation in seconds (default: 0.1)'
    )
    sort_parser.add_argument(
        '--seed',
        type=int,
        default=None,
        help='Random seed for reproducible arrays'
    )
    sort_parser.add_argument(
        '--no-stats',
        action='store_true',
        help='Hide statistics during visualization'
    )
    
    # ---- bench subcommand ----
    bench_parser = subparsers.add_parser(
        'bench',
        help='Run benchmarks comparing algorithms',
        description='Benchmark sorting algorithms across sizes and distributions'
    )
    bench_parser.add_argument(
        '-a', '--algorithms',
        nargs='+',
        type=validate_algorithm,
        default=list(ALGORITHMS.keys()),
        help='Algorithms to benchmark (default: all)'
    )
    bench_parser.add_argument(
        '--sizes',
        nargs='+',
        type=validate_positive_int,
        default=[100, 500, 1000],
        help='Array sizes for benchmark (default: 100 500 1000)'
    )
    bench_parser.add_argument(
        '--distributions',
        nargs='+',
        type=validate_distribution,
        default=['random', 'sorted', 'reversed', 'few-unique'],
        help='Distributions for benchmark (default: all)'
    )
    bench_parser.add_argument(
        '--trials',
        type=validate_positive_int,
        default=3,
        help='Number of trials per configuration (default: 3)'
    )
    bench_parser.add_argument(
        '--seed',
        type=int,
        default=None,
        help='Random seed for reproducible benchmarks'
    )
    
    # ---- list subcommand ----
    list_parser = subparsers.add_parser(
        'list',
        help='List available algorithms or distributions',
        description='List available sorting algorithms or input distributions'
    )
    list_parser.add_argument(
        'what',
        nargs='?',
        choices=['algorithms', 'distributions'],
        default='algorithms',
        help='What to list (default: algorithms)'
    )
    list_parser.add_argument(
        '--info',
        action='store_true',
        help='Show detailed algorithm information'
    )
    
    return parser


def cmd_sort(args: argparse.Namespace) -> int:
    """Execute the sort subcommand."""
    # Get the sorting function
    sort_func = ALGORITHMS[args.algorithm]
    
    # Generate the array with optional seed
    data = generate_array(args.distribution, args.size, seed=args.seed)
    
    seed_msg = f" (seed={args.seed})" if args.seed is not None else ""
    print(f"\nVisualizing {args.algorithm} sort on {args.distribution} array of size {args.size}{seed_msg}")
    print("Press Ctrl+C to stop the visualization\n")
    
    try:
        sorted_data = visualize_sorting(
            sort_func,
            data,
            speed=args.speed,
            show_stats=not args.no_stats
        )
        
        print(f"\nSorting complete!")
        if len(sorted_data) > 10:
            print(f"First 10 elements: {sorted_data[:10]}...")
        else:
            print(f"Result: {sorted_data}")
        
        return 0
        
    except KeyboardInterrupt:
        print("\nVisualization stopped by user.")
        return 130


def cmd_bench(args: argparse.Namespace) -> int:
    """Execute the bench subcommand."""
    if args.seed is not None:
        print(f"Using random seed: {args.seed}")
    
    print("\nRunning Benchmark...")
    print(f"Algorithms: {', '.join(args.algorithms)}")
    print(f"Sizes: {args.sizes}")
    print(f"Distributions: {', '.join(args.distributions)}")
    print(f"Trials: {args.trials}")
    
    # Get algorithms to benchmark
    algorithms = {name: ALGORITHMS[name] for name in args.algorithms}
    
    # Run benchmark
    results = run_benchmark(
        algorithms=algorithms,
        sizes=args.sizes,
        distributions=args.distributions,
        num_trials=args.trials,
        seed=args.seed
    )
    
    # Format and display results
    table = format_benchmark_table(results)
    print(table)
    
    # Show fastest algorithms
    print("\nFastest Algorithms:")
    print("-" * 40)
    for size in args.sizes:
        for dist in args.distributions:
            from .bench import get_fastest_algorithm
            fastest = get_fastest_algorithm(results, size, dist)
            print(f"  Size {size}, {dist}: {fastest}")
    
    return 0


def cmd_list(args: argparse.Namespace) -> int:
    """Execute the list subcommand."""
    if args.what == 'algorithms':
        if args.info:
            print("\nAvailable Algorithms:")
            print("=" * 60)
            for algo_name in sorted(ALGORITHMS.keys()):
                info = ALGORITHM_INFO.get(algo_name, {})
                print(f"\n  {algo_name}:")
                print(f"    Time Complexity:  {info.get('time_complexity', 'N/A')}")
                print(f"    Space Complexity: {info.get('space_complexity', 'N/A')}")
                print(f"    Stable: {'Yes' if info.get('stable', False) else 'No'}")
        else:
            print("\nAvailable Algorithms:")
            print("-" * 30)
            for algo_name in sorted(ALGORITHMS.keys()):
                print(f"  {algo_name}")
    
    elif args.what == 'distributions':
        print("\nAvailable Distributions:")
        print("-" * 30)
        for dist in DISTRIBUTIONS:
            print(f"  {dist}")
    
    return 0


def main(argv: Optional[List[str]] = None) -> int:
    """
    Main entry point for the CLI.
    
    Args:
        argv: Command line arguments (defaults to sys.argv[1:])
        
    Returns:
        Exit code (0 for success)
    """
    parser = build_parser()
    args = parser.parse_args(argv)
    
    # If no subcommand given, show help
    if args.command is None:
        parser.print_help()
        return 0
    
    # Dispatch to subcommand handler
    handlers = {
        'sort': cmd_sort,
        'bench': cmd_bench,
        'list': cmd_list,
    }
    
    handler = handlers.get(args.command)
    if handler is None:
        parser.print_help()
        return 1
    
    return handler(args)


if __name__ == '__main__':
    sys.exit(main())
