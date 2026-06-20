"""
Command-line interface for motif discovery.

Provides a CLI for running motif-finding algorithms on FASTA sequences.
"""

import argparse
import sys
import json
from typing import List, Optional

from bio_motif_finder.pwm import PWM
from bio_motif_finder.score import BackgroundModel, MotifScorer
from bio_motif_finder.greedy import GreedyMotifFinder
from bio_motif_finder.gibbs import GibbsSampler
from bio_motif_finder.meme import MEMELite
from bio_motif_finder.simulate import MotifSimulator


def parse_fasta(filepath: str) -> tuple:
    """
    Parse FASTA file.
    
    Args:
        filepath: Path to FASTA file.
    
    Returns:
        Tuple of (sequences, names).
    """
    sequences = []
    names = []
    current_seq = []
    current_name = None
    
    with open(filepath, 'r') as f:
        for line in f:
            line = line.strip()
            if line.startswith('>'):
                if current_name is not None:
                    sequences.append(''.join(current_seq))
                    names.append(current_name)
                
                current_name = line[1:].split()[0] if line[1:].strip() else f"seq_{len(sequences)}"
                current_seq = []
            elif line:
                current_seq.append(line.upper())
        
        if current_name is not None:
            sequences.append(''.join(current_seq))
            names.append(current_name)
    
    return sequences, names


def run_greedy(sequences: List[str], 
               motif_width: int,
               background: BackgroundModel) -> dict:
    """Run greedy algorithm."""
    finder = GreedyMotifFinder(
        motif_width=motif_width,
        background=background
    )
    return finder.find_motif(sequences)


def run_gibbs(sequences: List[str],
              motif_width: int,
              background: BackgroundModel,
              iterations: int = 1000) -> dict:
    """Run Gibbs sampling."""
    sampler = GibbsSampler(
        motif_width=motif_width,
        num_iterations=iterations,
        background=background
    )
    return sampler.find_motif(sequences, num_starts=5)


def run_meme(sequences: List[str],
             motif_width: int,
             background: BackgroundModel) -> dict:
    """Run MEME-lite algorithm."""
    meme = MEMELite(
        motif_width=motif_width,
        background=background
    )
    return meme.find_motif(sequences, num_starts=5)


def format_output(result: dict, 
                 sequences: Optional[List[str]] = None,
                 format_type: str = 'text') -> str:
    """
    Format output for display.
    
    Args:
        result: Algorithm results.
        sequences: Original sequences.
        format_type: Output format ('text', 'json', 'fasta').
    
    Returns:
        Formatted string.
    """
    if format_type == 'json':
        # Convert PWM to serializable format
        result_copy = result.copy()
        if 'pwm' in result_copy:
            result_copy['pwm'] = result_copy['pwm'].to_dict()
        return json.dumps(result_copy, indent=2)
    
    lines = []
    lines.append("=" * 60)
    lines.append("MOTIF DISCOVERY RESULTS")
    lines.append("=" * 60)
    lines.append("")
    lines.append(f"Algorithm: {result.get('method', 'unknown').upper()}")
    lines.append(f"Motif width: {len(result['consensus'])}")
    lines.append("")
    lines.append("Consensus sequence:")
    lines.append(f"  {result['consensus']}")
    lines.append("")
    
    # PWM data
    pwm = result['pwm']
    lines.append("Position Weight Matrix (probabilities):")
    lines.append("")
    lines.append("Position:  " + "  ".join(f"{i:3d}" for i in range(pwm.length)))
    lines.append("-" * (11 + pwm.length * 5))
    for nuc in ['A', 'C', 'G', 'T']:
        probs = [pwm.get_probability(nuc, j) for j in range(pwm.length)]
        lines.append(f"  {nuc}:     " + "  ".join(f"{p:.3f}" for p in probs))
    lines.append("")
    
    # Sites
    lines.append(f"Found {len(result['sites'])} motif sites:")
    lines.append("")
    for i, site_info in enumerate(result['sites']):
        seq_idx = site_info['sequence_index']
        pos = site_info['position']
        site = site_info['site']
        
        hamming = site_info.get('hamming_distance', 0)
        hamming_str = f" (Hamming: {hamming})" if hamming > 0 else ""
        
        if sequences:
            seq_display = sequences[seq_idx][:50] + "..." if len(sequences[seq_idx]) > 50 else sequences[seq_idx]
            lines.append(f"  {i+1:3d}. Sequence {seq_idx+1}, position {pos}:")
            lines.append(f"       {seq_display}")
            lines.append(f"       Site: {site}{hamming_str}")
        else:
            lines.append(f"  {i+1:3d}. Position {pos}: {site}{hamming_str}")
    lines.append("")
    
    # Logo data
    logo_data = pwm.weblogo_data()
    lines.append("Logo data (nucleotide heights):")
    for pos in range(pwm.length):
        heights = logo_data[pos]
        max_nuc = max(heights, key=heights.get)
        lines.append(f"  Position {pos}: {max_nuc} = {heights[max_nuc]:.3f}")
    
    lines.append("")
    lines.append("=" * 60)
    
    return '\n'.join(lines)


def main():
    """Main CLI entry point."""
    parser = argparse.ArgumentParser(
        description="DNA motif-discovery toolkit",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Find motifs in FASTA sequences
  motif-finder sequences.fasta --width 8
  
  # Use specific algorithm
  motif-finder sequences.fasta --width 10 --algorithm gibbs
  
  # Output as JSON
  motif-finder sequences.fasta --width 8 --format json
  
  # Generate test data and find motifs
  motif-finder --generate --width 8
        """
    )
    
    parser.add_argument('input', nargs='?', help='Input FASTA file')
    parser.add_argument('-w', '--width', type=int, default=8,
                       help='Motif width (default: 8)')
    parser.add_argument('-a', '--algorithm', 
                       choices=['greedy', 'gibbs', 'meme', 'auto'],
                       default='auto',
                       help='Algorithm to use (default: auto)')
    parser.add_argument('-f', '--format',
                       choices=['text', 'json', 'fasta'],
                       default='text',
                       help='Output format (default: text)')
    parser.add_argument('-o', '--output', help='Output file (default: stdout)')
    parser.add_argument('-i', '--iterations', type=int, default=1000,
                       help='Number of iterations for Gibbs sampling (default: 1000)')
    parser.add_argument('-s', '--seed', type=int,
                       help='Random seed for reproducibility')
    parser.add_argument('--generate', action='store_true',
                       help='Generate test data instead of reading input')
    parser.add_argument('--generate-count', type=int, default=20,
                       help='Number of sequences to generate (default: 20)')
    parser.add_argument('--generate-length', type=int, default=100,
                       help='Length of generated sequences (default: 100)')
    parser.add_argument('--motif', help='Specific motif to implant (for --generate)')
    parser.add_argument('--mutations', type=int, default=1,
                       help='Mutations per motif instance (default: 1)')
    
    args = parser.parse_args()
    
    # Generate test data if requested
    if args.generate:
        simulator = MotifSimulator(seed=args.seed)
        data = simulator.generate_dataset(
            num_sequences=args.generate_count,
            sequence_length=args.generate_length,
            motif_length=args.width,
            motif=args.motif,
            mutations_per_instance=args.mutations
        )
        sequences = data.sequences
        names = [f"seq_{i}" for i in range(len(sequences))]
        print(f"Generated {len(sequences)} sequences with motif: {data.motif}", file=sys.stderr)
    elif args.input:
        # Parse input file
        try:
            sequences, names = parse_fasta(args.input)
        except FileNotFoundError:
            print(f"Error: File not found: {args.input}", file=sys.stderr)
            sys.exit(1)
        except Exception as e:
            print(f"Error parsing FASTA: {e}", file=sys.stderr)
            sys.exit(1)
    else:
        print("Error: Please provide an input file or use --generate", file=sys.stderr)
        sys.exit(1)
    
    if not sequences:
        print("Error: No sequences found", file=sys.stderr)
        sys.exit(1)
    
    # Create background model
    background = BackgroundModel.from_sequences(sequences)
    
    # Select algorithm
    if args.algorithm == 'auto':
        # Auto-select based on motif width
        if args.width <= 8:
            algorithm = 'greedy'
        else:
            algorithm = 'gibbs'
    else:
        algorithm = args.algorithm
    
    print(f"Using algorithm: {algorithm}", file=sys.stderr)
    print(f"Motif width: {args.width}", file=sys.stderr)
    
    # Run algorithm
    try:
        if algorithm == 'greedy':
            result = run_greedy(sequences, args.width, background)
        elif algorithm == 'gibbs':
            result = run_gibbs(sequences, args.width, background, args.iterations)
        elif algorithm == 'meme':
            result = run_meme(sequences, args.width, background)
        else:
            print(f"Error: Unknown algorithm: {algorithm}", file=sys.stderr)
            sys.exit(1)
    except Exception as e:
        print(f"Error running algorithm: {e}", file=sys.stderr)
        sys.exit(1)
    
    # Format output
    output = format_output(result, sequences, args.format)
    
    # Write output
    if args.output:
        with open(args.output, 'w') as f:
            f.write(output)
        print(f"Results written to: {args.output}", file=sys.stderr)
    else:
        print(output)
    
    return 0


if __name__ == '__main__':
    sys.exit(main())
