// cli_main.cpp — CLI calculator reading arithmetic expressions
#include "bigint.hpp"
#include <iostream>
#include <string>
#include <sstream>
#include <cctype>

using namespace bigint;

// Simple recursive descent parser for expressions:
// expr = term (('+' | '-') term)*
// term = factor (('*' | '/' | '%') factor)*
// factor = ['-'] ( number | '(' expr ')' | 'pow(' expr ',' expr ')' | 'gcd(' expr ',' expr ')' )
// number = decimal digits | '0x' hex digits

struct Parser {
    std::string input;
    size_t pos;

    Parser(const std::string& s) : input(s), pos(0) {}

    void skip_ws() {
        while (pos < input.size() && std::isspace(input[pos])) ++pos;
    }

    char peek() {
        skip_ws();
        return pos < input.size() ? input[pos] : '\0';
    }

    char advance() {
        skip_ws();
        return pos < input.size() ? input[pos++] : '\0';
    }

    bool match(char c) {
        if (peek() == c) { ++pos; return true; }
        return false;
    }

    BigInt parse() {
        BigInt result = parse_expr();
        skip_ws();
        if (pos < input.size()) {
            throw std::runtime_error(std::string("unexpected character: ") + input[pos]);
        }
        return result;
    }

    BigInt parse_expr() {
        BigInt left = parse_term();
        while (true) {
            char c = peek();
            if (c == '+') { advance(); left = left + parse_term(); }
            else if (c == '-') { advance(); left = left - parse_term(); }
            else break;
        }
        return left;
    }

    BigInt parse_term() {
        BigInt left = parse_factor();
        while (true) {
            char c = peek();
            if (c == '*') { advance(); left = left * parse_factor(); }
            else if (c == '/') { advance(); left = left / parse_factor(); }
            else if (c == '%') { advance(); left = left % parse_factor(); }
            else break;
        }
        return left;
    }

    BigInt parse_factor() {
        skip_ws();

        // Unary minus
        bool neg = false;
        if (peek() == '-') { advance(); neg = true; }

        BigInt val;

        if (peek() == '(') {
            advance();
            val = parse_expr();
            if (advance() != ')') throw std::runtime_error("expected ')'");
        }
        else if (pos + 3 < input.size() && input.substr(pos, 4) == "pow(") {
            pos += 4;
            BigInt base = parse_expr();
            skip_ws();
            if (advance() != ',') throw std::runtime_error("expected ',' in pow()");
            BigInt exp = parse_expr();
            skip_ws();
            if (advance() != ')') throw std::runtime_error("expected ')' in pow()");
            val = BigInt::pow(base, exp.to_string().find('-') != std::string::npos ? 0 :
                             std::stoull(exp.to_string()));
        }
        else if (pos + 3 < input.size() && input.substr(pos, 4) == "gcd(") {
            pos += 4;
            BigInt a = parse_expr();
            skip_ws();
            if (advance() != ',') throw std::runtime_error("expected ',' in gcd()");
            BigInt b = parse_expr();
            skip_ws();
            if (advance() != ')') throw std::runtime_error("expected ')' in gcd()");
            val = BigInt::gcd(a, b);
        }
        else if (peek() == '0' && pos + 1 < input.size() && (input[pos+1] == 'x' || input[pos+1] == 'X')) {
            // Hex number
            std::string hex = "0x";
            pos += 2;
            while (pos < input.size() && std::isxdigit(input[pos])) hex += input[pos++];
            val = BigInt(hex);
        }
        else if (std::isdigit(peek())) {
            std::string num;
            while (pos < input.size() && std::isdigit(input[pos])) num += input[pos++];
            val = BigInt(num);
        }
        else {
            throw std::runtime_error(std::string("unexpected character: ") + peek());
        }

        return neg ? -val : val;
    }
};

int main() {
    std::cout << "BigInt Calculator\n";
    std::cout << "Operators: + - * / % | Functions: pow(a,b), gcd(a,b)\n";
    std::cout << "Numbers: decimal or 0x hex. Type 'quit' to exit.\n\n";

    std::string line;
    while (true) {
        std::cout << "> ";
        if (!std::getline(std::cin, line)) break;
        if (line == "quit" || line == "exit") break;
        if (line.empty()) continue;

        try {
            Parser p(line);
            BigInt result = p.parse();
            std::cout << "= " << result.to_string() << "\n";
            // Also show hex for small-ish numbers
            if (result.bit_length() <= 512 && !result.is_zero()) {
                std::cout << "= 0x" << result.to_hex_string() << "\n";
            }
        } catch (const std::exception& e) {
            std::cerr << "Error: " << e.what() << "\n";
        }
    }

    return 0;
}
