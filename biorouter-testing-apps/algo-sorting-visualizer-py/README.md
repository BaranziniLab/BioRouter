# Sorting Algorithm Visualizer

A comprehensive Python library implementing 9 sorting algorithms with animated terminal visualization and benchmarking capabilities.

## Features

- **9 Sorting Algorithms**: Bubble, Insertion, Selection, Merge, Quick (median-of-three), Heap, Shell, Counting, and Radix sort
- **Animated Visualization**: Real-time terminal animation with colored bars showing comparisons and swaps
- **Instrumentation Layer**: Counts comparisons, swaps, and array accesses
- **Benchmark Harness**: Compare algorithms across different input sizes and distributions
- **CLI Interface**: Easy-to-use command line interface for visualization and benchmarking
- **Comprehensive Tests**: Full test suite with edge cases and stability tests

## Algorithm Complexity

| Algorithm   | Time Complexity (Avg) | Time Complexity (Worst) | Space Complexity | Stable |
|-------------|----------------------|------------------------|------------------|--------|
| Bubble      | O(n²)               | O(n²)                  | O(1)             | Yes    |
| Insertion   | O(n²)               | O(n²)                  | O(1)             | Yes    |
| Selection   | O(n²)               | O(n²)                  | O(1)             | No     |
| Merge       | O(n log n)          | O(n log n)             | O(n)             | Yes    |
| Quick       | O(n log n)          | O(n²)                  | O(log n)         | No     |
| Heap        | O(n log n)          | O(n log n)             | O(1)             | No     |
| Shell       | O(n log²n)          | O(n log²n)             | O(1)             | No     |
| Counting    | O(n + k)            | O(n + k)               | O(n + k)         | Yes    |
| Radix       | O(d * (n + k))      | O(d * (n + k))         | O(n + k)         | Yes    |

Where:
- n = number of elements
- k = range of input (for counting/radix)
- d = number of digits (for radix)

## Installation

### From Source

```bash
git clone <repository-url>
cd algo-sorting-visualizer-py
pip install -e .
```

### Dependencies

- Python 3.8+
- No external dependencies for core functionality
- `windows-curses` for Windows terminal support (optional)

## Usage

### Command Line Interface

The CLI uses subcommands. Run `sorting-viz -h` or `sorting-viz <subcommand> -h` for help.

#### List Available Options

```bash
# List all algorithms
sorting-viz list

# List algorithms with detailed complexity info
sorting-viz list algorithms --info

# List available distributions
sorting-viz list distributions
```

#### Visualize an Algorithm (`sort`)

```bash
# Visualize bubble sort on random array of size 20
sorting-viz sort bubble -n 20

# Visualize quick sort on a sorted array with slow speed
sorting-viz sort quick -n 30 -d sorted -s 0.5

# Use --seed for reproducible arrays
sorting-viz sort merge -n 25 --seed 42

# Visualize with few-unique distribution
sorting-viz sort heap -n 30 -d few-unique --seed 123
```

#### Run Benchmarks (`bench`)

```bash
# Benchmark all algorithms on default sizes (100, 500, 1000)
sorting-viz bench

# Benchmark specific algorithms and distributions
sorting-viz bench -a bubble quick heap --distributions random sorted

# Custom sizes and trials with reproducible seed
sorting-viz bench --sizes 200 400 --trials 5 --seed 42
```

#### Unknown Algorithm Names

If you pass an unknown algorithm name, the CLI prints the available choices:

```
$ sorting-viz sort bogus
usage: sorting-viz sort [-h] ...
sorting-viz sort: error: argument algorithm: unknown algorithm 'bogus'. Available algorithms: bubble, counting, heap, insertion, merge, quick, radix, selection, shell
```

### Python API

#### Basic Usage

```python
from sorts import bubble_sort, quick_sort
from sorts.viz import visualize_sorting

# Visualize bubble sort
data = [64, 34, 25, 12, 22, 11, 90]
visualize_sorting(bubble_sort, data, speed=0.2)

# Get sorted result without visualization
sorted_data = []
for state in bubble_sort(data):
    sorted_data = state.array
print(sorted_data)
```

#### Benchmarking

```python
from sorts import bubble_sort, quick_sort, merge_sort
from sorts.bench import run_benchmark, format_benchmark_table

# Define algorithms to benchmark
algorithms = {
    'bubble': bubble_sort,
    'quick': quick_sort,
    'merge': merge_sort
}

# Run benchmark
results = run_benchmark(
    algorithms=algorithms,
    sizes=[100, 500, 1000],
    distributions=['random', 'sorted', 'reversed'],
    num_trials=3
)

# Display results
print(format_benchmark_table(results))
```

#### Instrumentation

```python
from sorts import bubble_sort
from sorts.instrument import instrument_sort, get_algorithm_info

# Get algorithm information
info = get_algorithm_info('bubble')
print(f"Time Complexity: {info['time_complexity']}")
print(f"Stable: {info['stable']}")

# Run with instrumentation
data = [64, 34, 25, 12, 22, 11, 90]
sorted_data, stats = instrument_sort(bubble_sort, data)
print(f"Comparisons: {stats.comparisons}")
print(f"Swaps: {stats.swaps}")
```

## Available Distributions

- **random**: Random integers between 0 and 2*size
- **sorted**: Already sorted array [0, 1, 2, ..., n-1]
- **reversed**: Reverse sorted array [n, n-1, ..., 1, 0]
- **few-unique**: Array with only 10 unique values

## Testing

### Run All Tests

```bash
pytest
```

### Run Specific Test Categories

```bash
# Test correctness
pytest tests/test_sorting.py::TestSortingCorrectness -v

# Test stability
pytest tests/test_sorting.py::TestStability -v

# Test edge cases
pytest tests/test_sorting.py::TestEdgeCases -v
```

### Run with Coverage

```bash
pytest --cov=sorts --cov-report=html
```

## Project Structure

```
algo-sorting-visualizer-py/
├── sorts/                    # Main package
│   ├── __init__.py          # Package initialization
│   ├── __main__.py          # Entry point for `python -m sorts`
│   ├── base.py              # Base classes and instrumentation
│   ├── bubble.py            # Bubble sort implementation
│   ├── insertion.py         # Insertion sort implementation
│   ├── selection.py         # Selection sort implementation
│   ├── merge.py             # Merge sort implementation
│   ├── quick.py             # Quick sort with median-of-three
│   ├── heap.py              # Heap sort implementation
│   ├── shell.py             # Shell sort implementation
│   ├── counting.py          # Counting sort implementation
│   ├── radix.py             # Radix sort implementation
│   ├── viz.py               # Terminal visualizer
│   ├── bench.py             # Benchmark harness
│   ├── instrument.py        # Instrumentation layer
│   └── cli.py               # Command line interface
├── tests/                   # Test suite
│   ├── __init__.py
│   └── test_sorting.py      # Comprehensive tests
├── pyproject.toml           # Project configuration
└── README.md                # This file
```

## Algorithm Details

### Bubble Sort
- **How it works**: Repeatedly steps through the list, compares adjacent elements, and swaps them if they are in the wrong order
- **Best for**: Small datasets, educational purposes
- **Worst case**: O(n²) when array is reverse sorted

### Insertion Sort
- **How it works**: Builds the final sorted array one item at a time by inserting each element into its correct position
- **Best for**: Small datasets, nearly sorted arrays
- **Worst case**: O(n²) when array is reverse sorted

### Selection Sort
- **How it works**: Repeatedly finds the minimum element from the unsorted portion and puts it at the beginning
- **Best for**: Small datasets, when memory writes are expensive
- **Worst case**: O(n²) for all cases

### Merge Sort
- **How it works**: Divides the array into halves, recursively sorts them, then merges the sorted halves
- **Best for**: Large datasets, external sorting, stable sort required
- **Worst case**: O(n log n) for all cases

### Quick Sort
- **How it works**: Picks a pivot (median-of-three), partitions the array around the pivot, recursively sorts the partitions
- **Best for**: General purpose, large datasets
- **Worst case**: O(n²) when pivot selection is poor (rare with median-of-three)

### Heap Sort
- **How it works**: Builds a max heap, then repeatedly extracts the maximum element
- **Best for**: Large datasets, guaranteed O(n log n) performance
- **Worst case**: O(n log n) for all cases

### Shell Sort
- **How it works**: Generalization of insertion sort that allows exchange of far apart elements
- **Best for**: Medium datasets, when simplicity is desired
- **Worst case**: O(n log²n) with Ciura's gap sequence

### Counting Sort
- **How it works**: Counts occurrences of each value, then uses counts to place elements in correct position
- **Best for**: Small range of integers, linear time required
- **Worst case**: O(n + k) where k is the range of input

### Radix Sort
- **How it works**: Sorts numbers digit by digit from least significant to most significant
- **Best for**: Fixed-length integers, linear time required
- **Worst case**: O(d * (n + k)) where d is number of digits

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- Inspired by various sorting algorithm visualizations
- Built with Python's built-in `random` module for array generation
- Uses ANSI escape codes for terminal visualization
