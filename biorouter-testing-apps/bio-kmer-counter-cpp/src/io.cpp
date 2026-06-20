/**
 * @file io.cpp
 * @brief Implementation of FASTA/FASTQ parser.
 */

#include "io.hpp"
#include <algorithm>
#include <stdexcept>
#include <cctype>

namespace bkc {

// --- Format detection ---

FileFormat detect_format(const std::string& filename) {
    // Check extension.
    std::string ext;
    auto dot = filename.rfind('.');
    if (dot != std::string::npos) {
        ext = filename.substr(dot);
        std::transform(ext.begin(), ext.end(), ext.begin(), ::tolower);
    }

    if (ext == ".fa" || ext == ".fasta" || ext == ".fna") {
        return FileFormat::FASTA;
    }
    if (ext == ".fq" || ext == ".fastq") {
        return FileFormat::FASTQ;
    }

    // Try content-based detection: peek at first character.
    std::ifstream ifs(filename);
    if (!ifs.is_open()) {
        return FileFormat::UNKNOWN;
    }

    char first = 0;
    while (ifs.get(first)) {
        if (first == '>') return FileFormat::FASTA;
        if (first == '@') return FileFormat::FASTQ;
        if (!std::isspace(first)) break;
    }

    return FileFormat::UNKNOWN;
}

// --- FASTA parsing ---

std::vector<SequenceRecord> parse_fasta(const std::string& filename) {
    std::ifstream ifs(filename);
    if (!ifs.is_open()) {
        throw std::runtime_error("Cannot open FASTA file: " + filename);
    }

    std::vector<SequenceRecord> records;
    SequenceRecord current;
    bool in_record = false;

    std::string line;
    while (std::getline(ifs, line)) {
        // Strip trailing \r
        if (!line.empty() && line.back() == '\r') {
            line.pop_back();
        }

        if (line.empty()) continue;

        if (line[0] == '>') {
            // Save previous record.
            if (in_record) {
                records.push_back(std::move(current));
                current = SequenceRecord{};
            }

            // Parse header.
            std::string header = line.substr(1);
            auto space_pos = header.find_first_of(" \t");
            if (space_pos != std::string::npos) {
                current.id = header.substr(0, space_pos);
                current.comment = header.substr(space_pos + 1);
            } else {
                current.id = header;
            }
            current.sequence.clear();
            current.quality.clear();
            in_record = true;
        } else if (in_record) {
            // Concatenate sequence line.
            current.sequence += line;
        }
    }

    // Save last record.
    if (in_record) {
        records.push_back(std::move(current));
    }

    return records;
}

// --- FASTQ parsing ---

std::vector<SequenceRecord> parse_fastq(const std::string& filename) {
    std::ifstream ifs(filename);
    if (!ifs.is_open()) {
        throw std::runtime_error("Cannot open FASTQ file: " + filename);
    }

    std::vector<SequenceRecord> records;
    enum State { HEADER, SEQUENCE, PLUS, QUALITY };
    State state = HEADER;

    SequenceRecord current;
    std::string line;

    while (std::getline(ifs, line)) {
        // Strip trailing \r
        if (!line.empty() && line.back() == '\r') {
            line.pop_back();
        }

        switch (state) {
            case HEADER:
                if (line.empty()) continue;
                if (line[0] != '@') {
                    throw std::runtime_error(
                        "Expected '@' header in FASTQ, got: " + line.substr(0, 40));
                }
                {
                    std::string header = line.substr(1);
                    auto space_pos = header.find_first_of(" \t");
                    if (space_pos != std::string::npos) {
                        current.id = header.substr(0, space_pos);
                        current.comment = header.substr(space_pos + 1);
                    } else {
                        current.id = header;
                    }
                }
                current.sequence.clear();
                current.quality.clear();
                state = SEQUENCE;
                break;

            case SEQUENCE:
                if (line[0] == '+') {
                    throw std::runtime_error(
                        "Empty sequence in FASTQ record: " + current.id);
                }
                current.sequence += line;
                state = PLUS;
                break;

            case PLUS:
                if (line[0] != '+') {
                    throw std::runtime_error(
                        "Expected '+' separator in FASTQ after sequence, got: " +
                        line.substr(0, 40));
                }
                state = QUALITY;
                break;

            case QUALITY:
                current.quality += line;
                if (current.quality.size() >= current.sequence.size()) {
                    records.push_back(std::move(current));
                    current = SequenceRecord{};
                    state = HEADER;
                }
                break;
        }
    }

    // Handle incomplete record at EOF.
    if (state == QUALITY && !current.id.empty()) {
        records.push_back(std::move(current));
    } else if (state != HEADER) {
        throw std::runtime_error("Truncated FASTQ record at end of file");
    }

    return records;
}

// --- Unified parser ---

std::vector<SequenceRecord> parse_file(const std::string& filename) {
    FileFormat fmt = detect_format(filename);
    switch (fmt) {
        case FileFormat::FASTA:
            return parse_fasta(filename);
        case FileFormat::FASTQ:
            return parse_fastq(filename);
        default:
            throw std::runtime_error(
                "Cannot detect format of file: " + filename);
    }
}

// --- Streaming parser ---

void for_each_record(const std::string& filename,
                     std::function<bool(const SequenceRecord&)> callback) {
    FileFormat fmt = detect_format(filename);

    if (fmt == FileFormat::FASTA) {
        std::ifstream ifs(filename);
        if (!ifs.is_open()) {
            throw std::runtime_error("Cannot open file: " + filename);
        }

        SequenceRecord current;
        std::string line;
        bool in_record = false;

        while (std::getline(ifs, line)) {
            if (!line.empty() && line.back() == '\r') line.pop_back();
            if (line.empty()) continue;

            if (line[0] == '>') {
                if (in_record) {
                    if (!callback(current)) return;
                    current = SequenceRecord{};
                }
                std::string header = line.substr(1);
                auto sp = header.find_first_of(" \t");
                if (sp != std::string::npos) {
                    current.id = header.substr(0, sp);
                    current.comment = header.substr(sp + 1);
                } else {
                    current.id = header;
                }
                current.sequence.clear();
                in_record = true;
            } else if (in_record) {
                current.sequence += line;
            }
        }
        if (in_record) callback(current);

    } else if (fmt == FileFormat::FASTQ) {
        auto records = parse_fastq(filename);
        for (auto& rec : records) {
            if (!callback(rec)) return;
        }
    } else {
        throw std::runtime_error("Cannot detect format: " + filename);
    }
}

std::string concat_sequences(const std::string& filename) {
    std::string result;
    for_each_record(filename, [&](const SequenceRecord& rec) {
        result += rec.sequence;
        return true;
    });
    return result;
}

} // namespace bkc
