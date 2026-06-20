/**
 * @file main.cpp
 * @brief CLI entry point for bio-kmer-counter.
 */

#include "cli.hpp"
#include <iostream>
#include <exception>

int main(int argc, char* argv[]) {
    try {
        auto config = bkc::parse_args(argc, argv);

        switch (config.command) {
            case bkc::CliConfig::Command::COUNT:
                return bkc::run_count(config);

            case bkc::CliConfig::Command::ASSEMBLE:
                return bkc::run_assemble(config);

            case bkc::CliConfig::Command::INFO:
                return bkc::run_info(config);

            case bkc::CliConfig::Command::HELP:
                bkc::print_help();
                return 0;

            case bkc::CliConfig::Command::VERSION:
                bkc::print_version();
                return 0;
        }

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << "\n";
        return 1;
    }

    return 0;
}
