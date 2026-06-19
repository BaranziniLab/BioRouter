/**
 * @file test_io.cpp
 * @brief Tests for FASTA/FASTQ parser.
 */

#include "test_framework.hpp"
#include "io.hpp"
#include <string>
#include <fstream>
#include <cstdio>

using namespace bkc;

// Helper: write a temp file and return its path.
static std::string write_temp_file(const std::string& content, const std::string& ext) {
    // Use tmpnam for simplicity (not ideal for production but fine for tests).
    std::string name = std::tmpnam(nullptr) + ext;
    std::ofstream ofs(name);
    ofs << content;
    ofs.close();
    return name;
}

// ========== Format detection tests ==========

TEST(detect_fasta_by_extension) {
    auto path = write_temp_file(">seq\nACGT\n", ".fa");
    FileFormat fmt = detect_format(path);
    std::remove(path.c_str());
    ASSERT_TRUE(fmt == FileFormat::FASTA);
}

TEST(detect_fastq_by_extension) {
    auto path = write_temp_file("@read\nACGT\n+\nIIII\n", ".fq");
    FileFormat fmt = detect_format(path);
    std::remove(path.c_str());
    ASSERT_TRUE(fmt == FileFormat::FASTQ);
}

// ========== FASTA parsing tests ==========

TEST(parse_fasta_single_record) {
    std::string content = ">seq1\nACGTACGT\n";
    auto path = write_temp_file(content, ".fa");
    auto records = parse_fasta(path);
    std::remove(path.c_str());

    ASSERT_EQ(records.size(), 1u);
    ASSERT_EQ(records[0].id, "seq1");
    ASSERT_EQ(records[0].sequence, "ACGTACGT");
}

TEST(parse_fasta_with_comment) {
    std::string content = ">seq1 some description\nACGT\n";
    auto path = write_temp_file(content, ".fa");
    auto records = parse_fasta(path);
    std::remove(path.c_str());

    ASSERT_EQ(records.size(), 1u);
    ASSERT_EQ(records[0].id, "seq1");
    ASSERT_EQ(records[0].comment, "some description");
    ASSERT_EQ(records[0].sequence, "ACGT");
}

TEST(parse_fasta_multiline) {
    std::string content = ">seq1\nACGT\nTGCA\nACGT\n";
    auto path = write_temp_file(content, ".fa");
    auto records = parse_fasta(path);
    std::remove(path.c_str());

    ASSERT_EQ(records.size(), 1u);
    ASSERT_EQ(records[0].sequence, "ACGTTGCAACGT");
}

TEST(parse_fasta_multiple_records) {
    std::string content = ">seq1\nACGT\n>seq2\nTGCA\n";
    auto path = write_temp_file(content, ".fa");
    auto records = parse_fasta(path);
    std::remove(path.c_str());

    ASSERT_EQ(records.size(), 2u);
    ASSERT_EQ(records[0].id, "seq1");
    ASSERT_EQ(records[0].sequence, "ACGT");
    ASSERT_EQ(records[1].id, "seq2");
    ASSERT_EQ(records[1].sequence, "TGCA");
}

TEST(parse_fasta_header_methods) {
    SequenceRecord rec;
    rec.id = "seq1";
    rec.comment = "description";
    ASSERT_EQ(rec.header(), "seq1 description");

    rec.comment.clear();
    ASSERT_EQ(rec.header(), "seq1");
}

// ========== FASTQ parsing tests ==========

TEST(parse_fastq_single_record) {
    std::string content =
        "@read1\n"
        "ACGTACGT\n"
        "+\n"
        "IIIIIIII\n";
    auto path = write_temp_file(content, ".fq");
    auto records = parse_fastq(path);
    std::remove(path.c_str());

    ASSERT_EQ(records.size(), 1u);
    ASSERT_EQ(records[0].id, "read1");
    ASSERT_EQ(records[0].sequence, "ACGTACGT");
    ASSERT_EQ(records[0].quality, "IIIIIIII");
}

TEST(parse_fastq_multiple_records) {
    std::string content =
        "@read1\nACGT\n+\nIIII\n"
        "@read2\nTGCA\n+\nJJJJ\n";
    auto path = write_temp_file(content, ".fq");
    auto records = parse_fastq(path);
    std::remove(path.c_str());

    ASSERT_EQ(records.size(), 2u);
    ASSERT_EQ(records[0].id, "read1");
    ASSERT_EQ(records[1].id, "read2");
}

TEST(parse_fastq_with_comment) {
    std::string content =
        "@read1 some info\nACGT\n+\nIIII\n";
    auto path = write_temp_file(content, ".fq");
    auto records = parse_fastq(path);
    std::remove(path.c_str());

    ASSERT_EQ(records[0].id, "read1");
    ASSERT_EQ(records[0].comment, "some info");
}

// ========== Unified parser tests ==========

TEST(parse_file_auto_detect_fasta) {
    std::string content = ">seq1\nACGT\n";
    auto path = write_temp_file(content, ".fasta");
    auto records = parse_file(path);
    std::remove(path.c_str());

    ASSERT_EQ(records.size(), 1u);
    ASSERT_EQ(records[0].sequence, "ACGT");
}

TEST(parse_file_auto_detect_fastq) {
    std::string content = "@read1\nACGT\n+\nIIII\n";
    auto path = write_temp_file(content, ".fastq");
    auto records = parse_file(path);
    std::remove(path.c_str());

    ASSERT_EQ(records.size(), 1u);
}

// ========== for_each_record tests ==========

TEST(for_each_record_fasta) {
    std::string content = ">seq1\nACGT\n>seq2\nTGCA\n";
    auto path = write_temp_file(content, ".fa");

    size_t count = 0;
    for_each_record(path, [&](const SequenceRecord& rec) {
        count++;
        return true;
    });
    std::remove(path.c_str());

    ASSERT_EQ(count, 2u);
}

TEST(for_each_record_early_stop) {
    std::string content = ">seq1\nACGT\n>seq2\nTGCA\n>seq3\nAAAA\n";
    auto path = write_temp_file(content, ".fa");

    size_t count = 0;
    for_each_record(path, [&](const SequenceRecord& rec) {
        count++;
        return count < 2;  // Stop after 2.
    });
    std::remove(path.c_str());

    ASSERT_EQ(count, 2u);
}

// ========== concat_sequences tests ==========

TEST(concat_sequences_fasta) {
    std::string content = ">seq1\nACGT\n>seq2\nTGCA\n";
    auto path = write_temp_file(content, ".fa");
    std::string result = concat_sequences(path);
    std::remove(path.c_str());

    ASSERT_EQ(result, "ACGTTGCA");
}
