#pragma once

/**
 * @file cli.hpp
 * @brief Command-line interface for bio-kmer-counter.
 *
 * Subcommands:
 *   count   - Count k-mers from a FASTA/FASTQ file and print histogram.
 *   assemble - Build de Bruijn graph and output contigs.
 *   info    - Show GC content and complexity statistics.
 */

#include <string>
#include <vector>

namespace bkc {

/**
 * @brief CLI configuration parsed from command-line arguments.
 */
struct CliConfig {
    enum class Command {
        COUNT,
        ASSEMBLE,
        INFO,
        HELP,
        VERSION
    };

    Command command = Command::HELP;
    std::string input_file;
    size_t k = 21;
    uint64_t min_coverage = 1;
    bool show_spectrum = true;
    bool verbose = false;
    size_t max_contigs = 0;   ///< 0 = no limit.
    size_t complexity_kmer = 3; ///< k for complexity measurement.
};

/**
 * @brief Parse command-line arguments into a CliConfig.
 *
 * @param argc  Argument count.
 * @param argv  Argument vector.
 * @return      Parsed configuration.
 */
CliConfig parse_args(int argc, char* argv[]);

/**
 * @brief Print help / usage message.
 */
void print_help();

/**
 * @brief Print version.
 */
void print_version();

/**
 * @brief Execute the "count" subcommand.
 */
int run_count(const CliConfig& config);

/**
 * @brief Execute the "assemble" subcommand.
 */
int run_assemble(const CliConfig& config);

/**
 * @brief Execute the "info" subcommand.
 */
int run_info(const CliConfig& config);

} // namespace bkc
