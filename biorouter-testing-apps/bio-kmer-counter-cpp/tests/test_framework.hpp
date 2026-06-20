#pragma once

/**
 * @file test_framework.hpp
 * @brief Lightweight assertion-based test framework.
 *
 * Provides:
 *   TEST(name)           - Define a test case.
 *   ASSERT_EQ(a, b)      - Assert equality.
 *   ASSERT_NE(a, b)      - Assert inequality.
 *   ASSERT_TRUE(expr)    - Assert truth.
 *   ASSERT_FALSE(expr)   - Assert falseness.
 *   ASSERT_NEAR(a,b,eps) - Assert approximate equality.
 *   ASSERT_THROWS(expr, ExType) - Assert exception thrown.
 *   RUN_ALL_TESTS()      - Run all registered tests and report results.
 */

#include <iostream>
#include <vector>
#include <string>
#include <functional>
#include <cmath>
#include <sstream>

namespace bkc_test {

struct TestCase {
    std::string name;
    std::function<void()> func;
};

inline std::vector<TestCase>& get_tests() {
    static std::vector<TestCase> tests;
    return tests;
}

inline int& get_fail_count() {
    static int fails = 0;
    return fails;
}

inline int& get_pass_count() {
    static int passes = 0;
    return passes;
}

inline void record_failure(const char* expr, const char* file, int line,
                           const std::string& detail = "") {
    std::cerr << "  FAIL: " << expr << "\n";
    if (!detail.empty()) {
        std::cerr << "        " << detail << "\n";
    }
    std::cerr << "        at " << file << ":" << line << "\n";
    get_fail_count()++;
}

struct TestRegistrar {
    TestRegistrar(const std::string& name, std::function<void()> func) {
        get_tests().push_back({name, std::move(func)});
    }
};

#define TEST(name) \
    static void test_##name(); \
    static ::bkc_test::TestRegistrar reg_##name(#name, test_##name); \
    static void test_##name()

#define ASSERT_EQ(a, b) do { \
    auto _a = (a); auto _b = (b); \
    if (_a != _b) { \
        std::ostringstream _ss; \
        _ss << "Expected " << #a << " == " << #b << "\n" \
            << "  Got: " << _a << " vs " << _b; \
        ::bkc_test::record_failure(#a " == " #b, __FILE__, __LINE__, _ss.str()); \
        return; \
    } else { ::bkc_test::get_pass_count()++; } \
} while(0)

#define ASSERT_NE(a, b) do { \
    auto _a = (a); auto _b = (b); \
    if (_a == _b) { \
        std::ostringstream _ss; \
        _ss << "Expected " << #a << " != " << #b << "\n" \
            << "  Both: " << _a; \
        ::bkc_test::record_failure(#a " != " #b, __FILE__, __LINE__, _ss.str()); \
        return; \
    } else { ::bkc_test::get_pass_count()++; } \
} while(0)

#define ASSERT_TRUE(expr) do { \
    if (!(expr)) { \
        ::bkc_test::record_failure(#expr, __FILE__, __LINE__, "Expression was false"); \
        return; \
    } else { ::bkc_test::get_pass_count()++; } \
} while(0)

#define ASSERT_FALSE(expr) do { \
    if ((expr)) { \
        ::bkc_test::record_failure(#expr, __FILE__, __LINE__, "Expression was true"); \
        return; \
    } else { ::bkc_test::get_pass_count()++; } \
} while(0)

#define ASSERT_NEAR(a, b, eps) do { \
    double _a = static_cast<double>(a); \
    double _b = static_cast<double>(b); \
    double _eps = static_cast<double>(eps); \
    if (std::abs(_a - _b) > _eps) { \
        std::ostringstream _ss; \
        _ss << "Expected " << #a << " ~ " << #b << " (eps=" << _eps << ")\n" \
            << "  Got: " << _a << " vs " << _b << " (diff=" << std::abs(_a-_b) << ")"; \
        ::bkc_test::record_failure(#a " ~ " #b, __FILE__, __LINE__, _ss.str()); \
        return; \
    } else { ::bkc_test::get_pass_count()++; } \
} while(0)

#define ASSERT_THROWS(expr, ExType) do { \
    bool _threw = false; \
    try { expr; } catch (const ExType&) { _threw = true; } \
    if (!_threw) { \
        ::bkc_test::record_failure(#expr, __FILE__, __LINE__, \
            "Expected exception " #ExType " was not thrown"); \
        return; \
    } else { ::bkc_test::get_pass_count()++; } \
} while(0)

inline int RUN_ALL_TESTS() {
    auto& tests = get_tests();
    int total = tests.size();
    int passed = 0;
    int failed = 0;

    std::cout << "Running " << total << " test(s)...\n\n";

    for (auto& tc : tests) {
        std::cout << "  [RUN]  " << tc.name << "\n";
        int before_fail = get_fail_count();
        int before_pass = get_pass_count();
        try {
            tc.func();
        } catch (const std::exception& e) {
            std::cerr << "  FAIL:  Unhandled exception: " << e.what() << "\n";
            get_fail_count()++;
        } catch (...) {
            std::cerr << "  FAIL:  Unknown exception\n";
            get_fail_count()++;
        }

        if (get_fail_count() == before_fail) {
            std::cout << "  [PASS] " << tc.name << " (" << (get_pass_count() - before_pass) << " assertions)\n";
            passed++;
        } else {
            std::cout << "  [FAIL] " << tc.name << "\n";
            failed++;
        }
    }

    std::cout << "\n" << std::string(50, '=') << "\n";
    std::cout << "Results: " << passed << "/" << total << " tests passed";
    if (failed > 0) {
        std::cout << " (" << failed << " FAILED)";
    }
    std::cout << "\n";
    std::cout << "Assertions: " << get_pass_count() << " passed, "
              << get_fail_count() << " failed\n";

    return (failed > 0) ? 1 : 0;
}

} // namespace bkc_test
