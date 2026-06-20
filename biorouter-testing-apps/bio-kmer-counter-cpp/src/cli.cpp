/**
 * @file cli.cpp
 * @brief Implementation of the command-line interface.
 */

#include "cli.hpp"
#include "kmer.hpp"
#include "counter.hpp"
#include "dbg.hpp"
#include "io.hpp"
#include <iostream>
#include <fstream>
#include <sstream>
#include <iomanip>
#include <algorithm>
#include <stdexcept>

namespace bkc {

static constexpr const char* VERSION = "1.0.0";

CliConfig parse_args(int argc, char* argv[]) {
    CliConfig config;

    if (argc < 2) {
        config.command = CliConfig::Command::HELP;
        return config;
    }

    std::string cmd = argv[1];

    if (cmd == "count") {
        config.command = CliConfig::Command::COUNT;
    } else if (cmd == "assemble") {
        config.command = CliConfig::Command::ASSEMBLE;
    } else if (cmd == "info") {
        config.command = CliConfig::Command::INFO;
    } else if (cmd == "help" || cmd == "-h" || cmd == "--help") {
        config.command = CliConfig::Command::HELP;
        return config;
    } else if (cmd == "version" || cmd == "-v" || cmd == "--version") {
        config.command = CliConfig::Command::VERSION;
        return config;
    } else {
        throw std::runtime_error("Unknown command: " + cmd);
    }

    // Parse remaining arguments.
    for (int i = 2; i < argc; ++i) {
        std::string arg = argv[i];

        if ((arg == "-k" || arg == "--kmer") && i + 1 < argc) {
            config.k = std::stoul(argv[++i]);
        } else if ((arg == "-c" || arg == "--min-coverage") && i + 1 < argc) {
            config.min_coverage = std::stoul(argv[++i]);
        } else if ((arg == "-n" || arg == "--max-contigs") && i + 1 < argc) {
            config.max_contigs = std::stoul(argv[++i]);
        } else if (arg == "-v" || arg == "--verbose") {
            config.verbose = true;
        } else if (arg == "--no-spectrum") {
            config.show_spectrum = false;
        } else if (arg[0] != '-') {
            config.input_file = arg;
        } else {
            throw std::runtime_error("Unknown option: " + arg);
        }
    }

    if (config.input_file.empty() && config.command != CliConfig::Command::HELP &&
        config.command != CliConfig::Command::VERSION) {
        throw std::runtime_error("No input file specified. Use -h for help.");
    }

    return config;
}

void print_help() {
    std::cout << "bio-kmer-counter v" << VERSION << "\n"
              << "\n"
              << "A k-mer counting and de Bruijn graph toolkit.\n"
              << "\n"
              << "Usage:\n"
              << "  bio-kmer-counter <command> [options] <input.fa|fq>\n"
              << "\n"
              << "Commands:\n"
              << "  count       Count k-mers and print frequency spectrum\n"
              << "  assemble    Build de Bruijn graph and output contigs\n"
              << "  info        Show GC content and complexity statistics\n"
              << "  help        Show this help message\n"
              << "  version     Show version\n"
              << "\n"
              << "Options:\n"
              << "  -k, --kmer <int>          k-mer size (default: 21)\n"
              << "  -c, --min-coverage <int>  Minimum k-mer count (default: 1)\n"
              << "  -n, --max-contigs <int>   Maximum number of contigs (0=all)\n"
              << "  --no-spectrum             Suppress histogram output\n"
              << "  -v, --verbose             Verbose output\n"
              << "  -h, --help                Show this help\n"
              << "\n"
              << "Input formats: FASTA (.fa, .fasta, .fna), FASTQ (.fq, .fastq)\n"
              << "Multi-line sequences are handled automatically.\n";
}

void print_version() {
    std::cout << "bio-kmer-counter " << VERSION << "\n";
}

// --- Count subcommand ---

int run_count(const CliConfig& config) {
    if (config.verbose) {
        std::cerr << "[bio-kmer-counter] k=" << config.k
                  << " file=" << config.input_file << "\n";
    }

    // Parse input.
    auto records = parse_file(config.input_file);
    if (records.empty()) {
        std::cerr << "Warning: no sequences found in input file.\n";
        return 0;
    }

    if (config.verbose) {
        std::cerr << "[bio-kmer-counter] " << records.size() << " sequence(s) loaded.\n";
    }

    // Count.
    KmerCounter counter(config.k);
    for (auto& rec : records) {
        counter.count(rec.sequence);
    }

    // Print summary.
    std::cout << "=== k-mer Count Summary ===\n";
    std::cout << "k:             " << counter.k() << "\n";
    std::cout << "Total k-mers:  " << counter.total_count() << "\n";
    std::cout << "Unique k-mers: " << counter.unique_count() << "\n";
    std::cout << "Max count:     " << counter.max_count() << "\n";
    std::cout << "\n";

    // Print spectrum (histogram).
    if (config.show_spectrum) {
        auto spectrum = counter.spectrum();
        std::cout << "=== k-mer Frequency Spectrum ===\n";
        std::cout << std::setw(12) << "Count" << std::setw(15) << "Frequency" << "\n";
        std::cout << std::string(27, '-') << "\n";

        for (auto& entry : spectrum) {
            std::cout << std::setw(12) << entry.count
                      << std::setw(15) << entry.frequency << "\n";
        }
    }

    return 0;
}

// --- Assemble subcommand ---

int run_assemble(const CliConfig& config) {
    if (config.verbose) {
        std::cerr << "[bio-kmer-counter] Assembling with k=" << config.k
                  << " min-cov=" << config.min_coverage
                  << " file=" << config.input_file << "\n";
    }

    // Parse input.
    auto records = parse_file(config.input_file);
    if (records.empty()) {
        std::cerr << "Warning: no sequences found in input file.\n";
        return 0;
    }

    // Count.
    KmerCounter counter(config.k);
    for (auto& rec : records) {
        counter.count(rec.sequence);
    }

    if (config.verbose) {
        std::cerr << "[bio-kmer-counter] " << counter.unique_count()
                  << " unique k-mers, " << counter.total_count() << " total.\n";
    }

    // Build de Bruijn graph.
    DeBruijnGraph graph(config.k);
    graph.build(counter, config.min_coverage);

    if (config.verbose) {
        auto gs = graph.stats();
        std::cerr << "[bio-kmer-counter] Graph: " << gs.num_nodes << " nodes, "
                  << gs.num_edges << " edges.\n";
    }

    // Assemble.
    auto contigs = graph.assemble();

    if (contigs.empty()) {
        std::cout << "No contigs assembled.\n";
        return 0;
    }

    // Sort by length descending.
    std::sort(contigs.begin(), contigs.end(),
              [](const Contig& a, const Contig& b) { return a.length > b.length; });

    // Print contigs.
    size_t n_contigs = (config.max_contigs > 0) ?
        std::min(config.max_contigs, contigs.size()) : contigs.size();

    std::cout << "=== Assembled Contigs ===\n";
    std::cout << "Number of contigs: " << n_contigs << "\n\n";

    // Print stats.
    size_t total_len = 0;
    size_t max_len = 0;
    for (size_t i = 0; i < n_contigs; ++i) {
        total_len += contigs[i].length;
        max_len = std::max(max_len, contigs[i].length);
    }
    std::cout << "Total length: " << total_len << " bp\n";
    std::cout << "Largest contig: " << max_len << " bp\n";
    std::cout << "\n";

    // FASTA output.
    for (size_t i = 0; i < n_contigs; ++i) {
        auto& c = contigs[i];
        std::cout << ">contig_" << (i + 1)
                  << " length=" << c.length
                  << " kmer_count=" << c.kmer_count
                  << " avg_coverage=" << std::fixed << std::setprecision(1)
                  << c.avg_coverage << "\n";

        // Wrap at 80 columns.
        for (size_t pos = 0; pos < c.sequence.size(); pos += 80) {
            std::cout << c.sequence.substr(pos, 80) << "\n";
        }
    }

    return 0;
}

// --- Info subcommand ---

int run_info(const CliConfig& config) {
    if (config.verbose) {
        std::cerr << "[bio-kmer-counter] Analyzing file=" << config.input_file << "\n";
    }

    auto records = parse_file(config.input_file);
    if (records.empty()) {
        std::cerr << "Warning: no sequences found.\n";
        return 0;
    }

    size_t total_length = 0;
    size_t num_records = records.size();

    std::cout << "=== Sequence Info ===\n";
    std::cout << "File:          " << config.input_file << "\n";
    std::cout << "Sequences:     " << num_records << "\n\n";

    double total_gc = 0.0;
    for (auto& rec : records) {
        double gc = gc_content(rec.sequence);
        double cx = sequence_complexity(rec.sequence, config.complexity_kmer);

        std::cout << "  " << rec.id << "\n"
                  << "    Length:    " << rec.sequence.size() << " bp\n"
                  << "    GC:       " << std::fixed << std::setprecision(2)
                  << (gc * 100.0) << "%\n"
                  << "    Complexity (k=" << config.complexity_kmer << "): "
                  << std::fixed << std::setprecision(4) << cx << "\n\n";

        total_gc += gc * rec.sequence.size();
        total_length += rec.sequence.size();
    }

    if (total_length > 0) {
        std::cout << "=== Summary ===\n";
        std::cout << "Total length:  " << total_length << " bp\n";
        std::cout << "Overall GC:    " << std::fixed << std::setprecision(2)
                  << (total_gc / total_length * 100.0) << "%\n";
    }

    return 0;
}

} // namespace bkc
