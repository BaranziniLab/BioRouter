#pragma once

/**
 * @file io.hpp
 * @brief Simple FASTA and FASTQ parser.
 *
 * Supports both FASTA (.fa, .fasta) and FASTQ (.fq, .fastq) formats.
 * Multi-line sequences are concatenated automatically.
 */

#include <string>
#include <vector>
#include <fstream>
#include <optional>
#include <functional>

namespace bkc {

/**
 * @brief A single sequence record (from FASTA or FASTQ).
 */
struct SequenceRecord {
    std::string id;        ///< Identifier (without '>' or '@').
    std::string comment;   ///< Optional comment after whitespace on header line.
    std::string sequence;  ///< DNA sequence.
    std::string quality;   ///< Quality string (FASTQ only, empty for FASTA).

    /**
     * @brief Full header line (id + comment).
     */
    std::string header() const {
        if (comment.empty()) return id;
        return id + " " + comment;
    }
};

/**
 * @brief Detected file format.
 */
enum class FileFormat {
    FASTA,
    FASTQ,
    UNKNOWN
};

/**
 * @brief Detect file format from extension or content.
 */
FileFormat detect_format(const std::string& filename);

/**
 * @brief Parse all records from a FASTA or FASTQ file.
 *
 * @param filename  Path to input file.
 * @return          Vector of parsed records.
 * @throws std::runtime_error on I/O or format errors.
 */
std::vector<SequenceRecord> parse_file(const std::string& filename);

/**
 * @brief Parse a FASTA file.
 */
std::vector<SequenceRecord> parse_fasta(const std::string& filename);

/**
 * @brief Parse a FASTQ file.
 */
std::vector<SequenceRecord> parse_fastq(const std::string& filename);

/**
 * @brief Process records one at a time via a callback (memory-efficient for large files).
 *
 * @param filename  Path to input file.
 * @param callback  Function called for each record. Return false to stop.
 */
void for_each_record(const std::string& filename,
                     std::function<bool(const SequenceRecord&)> callback);

/**
 * @brief Concatenate all sequences from a file into a single string.
 *
 * Useful for feeding into KmerCounter.
 */
std::string concat_sequences(const std::string& filename);

} // namespace bkc
