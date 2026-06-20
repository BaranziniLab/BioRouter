"""
Unit tests for motif simulation.
"""

import pytest
import os
import tempfile

from bio_motif_finder.simulate import MotifSimulator, PlantedMotif, create_test_file


class TestMotifSimulator:
    """Tests for MotifSimulator class."""
    
    def test_generate_random_sequence(self, simulator):
        """Test random sequence generation."""
        seq = simulator.generate_random_sequence(50)
        
        assert len(seq) == 50
        assert all(nuc in 'ACGT' for nuc in seq)
    
    def test_mutate_sequence(self, simulator):
        """Test sequence mutation."""
        original = "ATCGATCG"
        mutated = simulator.mutate_sequence(original, 2)
        
        assert len(mutated) == len(original)
        
        # Count differences
        differences = sum(c1 != c2 for c1, c2 in zip(original, mutated))
        assert differences <= 2
    
    def test_mutate_sequence_zero_mutations(self, simulator):
        """Test mutation with zero changes."""
        original = "ATCGATCG"
        mutated = simulator.mutate_sequence(original, 0)
        
        assert mutated == original
    
    def test_implant_motif(self, simulator):
        """Test motif implantation."""
        sequences = ["AAAAAAAAAA", "CCCCCCCCCC", "GGGGGGGGGG"]
        motif = "ATCG"
        
        result = simulator.implant_motif(sequences, motif, mutations_per_instance=0)
        
        assert isinstance(result, PlantedMotif)
        assert result.motif == motif
        assert len(result.sequences) == 3
        assert len(result.positions) == 3
        
        # Each sequence should contain the motif
        for seq in result.sequences:
            assert motif in seq
    
    def test_implant_motif_with_mutations(self, simulator):
        """Test motif implantation with mutations."""
        sequences = ["AAAAAAAAAA", "CCCCCCCCCC", "GGGGGGGGGG"]
        motif = "ATCG"
        
        result = simulator.implant_motif(sequences, motif, mutations_per_instance=1)
        
        # Motif instances should differ from original by at most 1
        for i, seq in enumerate(result.sequences):
            pos = result.positions[i]
            instance = seq[pos:pos + len(motif)]
            
            differences = sum(c1 != c2 for c1, c2 in zip(motif, instance))
            assert differences <= 1
    
    def test_generate_dataset(self, simulator):
        """Test complete dataset generation."""
        data = simulator.generate_dataset(
            num_sequences=10,
            sequence_length=50,
            motif_length=6,
            motif="ATCGAT",
            mutations_per_instance=1
        )
        
        assert isinstance(data, PlantedMotif)
        assert len(data.sequences) == 10
        assert all(len(seq) == 50 for seq in data.sequences)
        assert data.motif == "ATCGAT"
    
    def test_generate_dataset_random_motif(self, simulator):
        """Test dataset generation with random motif."""
        data = simulator.generate_dataset(
            num_sequences=5,
            sequence_length=30,
            motif_length=4
        )
        
        assert len(data.motif) == 4
        assert all(nuc in 'ACGT' for nuc in data.motif)


class TestFASTAOperations:
    """Tests for FASTA parsing and generation."""
    
    def test_generate_fasta(self, simulator):
        """Test FASTA generation."""
        sequences = ["ATCGATCG", "GCGCGCGC"]
        fasta = simulator.generate_fasta(sequences)
        
        assert ">seq_0" in fasta
        assert ">seq_1" in fasta
        assert "ATCGATCG" in fasta
        assert "GCGCGCGC" in fasta
    
    def test_generate_fasta_with_names(self, simulator):
        """Test FASTA generation with custom names."""
        sequences = ["ATCGATCG", "GCGCGCGC"]
        names = ["gene1", "gene2"]
        fasta = simulator.generate_fasta(sequences, names)
        
        assert ">gene1" in fasta
        assert ">gene2" in fasta
    
    def test_parse_fasta(self, simulator):
        """Test FASTA parsing."""
        fasta_string = """>seq1
ATCGATCG
>seq2
GCGCGCGC"""
        
        sequences, names = simulator.parse_fasta(fasta_string)
        
        assert len(sequences) == 2
        assert len(names) == 2
        assert sequences[0] == "ATCGATCG"
        assert sequences[1] == "GCGCGCGC"
    
    def test_parse_fasta_multiline(self, simulator):
        """Test parsing multiline FASTA."""
        fasta_string = """>seq1
ATCG
ATCG
>seq2
GCGC
GCGC"""
        
        sequences, names = simulator.parse_fasta(fasta_string)
        
        assert len(sequences) == 2
        assert sequences[0] == "ATCGATCG"
        assert sequences[1] == "GCGCGCGC"
    
    def test_roundtrip_fasta(self, simulator):
        """Test FASTA roundtrip (generate then parse)."""
        original_sequences = ["ATCGATCG", "GCGCGCGC", "TTTTAAAA"]
        
        fasta = simulator.generate_fasta(original_sequences)
        parsed_sequences, _ = simulator.parse_fasta(fasta)
        
        assert parsed_sequences == original_sequences


class TestCreateTestFile:
    """Tests for test file creation."""
    
    def test_create_test_file(self):
        """Test test file creation."""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = os.path.join(tmpdir, "test.fasta")
            
            motif = create_test_file(filepath, num_sequences=5, sequence_length=50, motif_length=6)
            
            assert os.path.exists(filepath)
            assert len(motif) == 6
            
            # Read and verify
            with open(filepath, 'r') as f:
                content = f.read()
            
            assert ">seq_0" in content
            assert len(content) > 0


class TestPlantedMotif:
    """Tests for PlantedMotif dataclass."""
    
    def test_planted_motif_creation(self):
        """Test PlantedMotif creation."""
        pm = PlantedMotif(
            motif="ATCG",
            positions=[10, 20, 30],
            sequences=["seq1", "seq2", "seq3"],
            mutations=1
        )
        
        assert pm.motif == "ATCG"
        assert len(pm.positions) == 3
        assert pm.mutations == 1
