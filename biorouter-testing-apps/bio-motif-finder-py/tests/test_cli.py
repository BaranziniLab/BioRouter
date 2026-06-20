"""
Unit tests for command-line interface.
"""

import pytest
import os
import tempfile

from bio_motif_finder.cli import parse_fasta, format_output, main
from bio_motif_finder.simulate import MotifSimulator


class TestParseFasta:
    """Tests for FASTA parsing."""
    
    def test_parse_fasta_simple(self):
        """Test simple FASTA parsing."""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.fasta', delete=False) as f:
            f.write(">seq1\nATCGATCG\n>seq2\nGCGCGCGC\n")
            f.flush()
            filepath = f.name
        
        try:
            sequences, names = parse_fasta(filepath)
            
            assert len(sequences) == 2
            assert names == ["seq1", "seq2"]
            assert sequences[0] == "ATCGATCG"
            assert sequences[1] == "GCGCGCGC"
        finally:
            os.unlink(filepath)
    
    def test_parse_fasta_multiline(self):
        """Test multiline FASTA parsing."""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.fasta', delete=False) as f:
            f.write(">seq1\nATCG\nATCG\n>seq2\nGCGC\nGCGC\n")
            f.flush()
            filepath = f.name
        
        try:
            sequences, names = parse_fasta(filepath)
            
            assert sequences[0] == "ATCGATCG"
            assert sequences[1] == "GCGCGCGC"
        finally:
            os.unlink(filepath)
    
    def test_parse_fasta_nonexistent(self):
        """Test parsing nonexistent file."""
        with pytest.raises(FileNotFoundError):
            parse_fasta("nonexistent.fasta")


class TestFormatOutput:
    """Tests for output formatting."""
    
    def test_format_text(self):
        """Test text output format."""
        from bio_motif_finder.pwm import PWM as _PWM
        result = {
            'method': 'greedy',
            'consensus': 'ATCGATCG',
            'sites': [
                {'sequence_index': 0, 'position': 10, 'site': 'ATCGATCG', 'hamming_distance': 0}
            ],
            'pwm': _PWM.from_sequences(["ATCGATCG"] * 5)
        }
        
        output = format_output(result, format_type='text')
        
        assert "MOTIF DISCOVERY RESULTS" in output
        assert "ATCGATCG" in output
        assert "greedy" in output.lower()
    
    def test_format_json(self):
        """Test JSON output format."""
        from bio_motif_finder.pwm import PWM
        
        result = {
            'method': 'gibbs',
            'consensus': 'ATCG',
            'sites': [{'sequence_index': 0, 'position': 5, 'site': 'ATCG'}],
            'pwm': PWM.from_sequences(["ATCG"] * 5)
        }
        
        output = format_output(result, format_type='json')
        
        # Should be valid JSON
        import json
        parsed = json.loads(output)
        assert 'consensus' in parsed
    
    def test_format_with_sequences(self):
        """Test output with sequences."""
        from bio_motif_finder.pwm import PWM as _PWM
        result = {
            'method': 'meme',
            'consensus': 'ATCG',
            'sites': [{'sequence_index': 0, 'position': 5, 'site': 'ATCG'}],
            'pwm': _PWM.from_sequences(["ATCG"] * 5)
        }
        sequences = ["XXXATCGXXX"]
        
        output = format_output(result, sequences, format_type='text')
        
        assert "XXXATCGXXX" in output


class TestCLIIntegration:
    """Integration tests for CLI."""
    
    def test_cli_generate(self):
        """Test CLI with --generate flag."""
        result = os.system("python -m bio_motif_finder.cli --generate --width 6 --generate-count 5 --generate-length 50 -f json > /dev/null 2>&1")
        
        # Should run without error
        assert result == 0
    
    def test_cli_with_fasta(self):
        """Test CLI with FASTA file."""
        # Create temporary FASTA file
        with tempfile.NamedTemporaryFile(mode='w', suffix='.fasta', delete=False) as f:
            simulator = MotifSimulator(seed=42)
            data = simulator.generate_dataset(
                num_sequences=5,
                sequence_length=50,
                motif_length=6
            )
            fasta = simulator.generate_fasta(data.sequences)
            f.write(fasta)
            f.flush()
            filepath = f.name
        
        try:
            result = os.system(f"python -m bio_motif_finder.cli {filepath} --width 6 -f text > /dev/null 2>&1")
            
            # Should run without error
            assert result == 0
        finally:
            os.unlink(filepath)
    
    def test_cli_output_file(self):
        """Test CLI with output file."""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as out_f:
            outpath = out_f.name
        
        try:
            result = os.system(f"python -m bio_motif_finder.cli --generate --width 6 --generate-count 5 -o {outpath} > /dev/null 2>&1")
            
            assert result == 0
            assert os.path.exists(outpath)
            
            with open(outpath, 'r') as f:
                content = f.read()
            
            assert "MOTIF DISCOVERY RESULTS" in content
        finally:
            os.unlink(outpath)
