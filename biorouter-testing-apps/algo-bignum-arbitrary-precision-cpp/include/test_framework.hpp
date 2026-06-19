#pragma once

// Simple assertion-based test framework for BigInt

#include <iostream>
#include <string>
#include <vector>
#include <functional>
#include <sstream>
#include <cmath>

namespace test {

struct TestResult {
    std::string name;
    bool passed;
    std::string message;
};

inline std::vector<TestResult>& results() {
    static std::vector<TestResult> r;
    return r;
}

inline int total_tests() { return static_cast<int>(results().size()); }
inline int passed_tests() {
    int n = 0;
    for (auto& r : results()) if (r.passed) ++n;
    return n;
}
inline int failed_tests() { return total_tests() - passed_tests(); }

inline void record(const std::string& name, bool passed, const std::string& msg = "") {
    results().push_back({name, passed, msg});
}

// --- Assertions ---

#define TEST_ASSERT(expr) \
    do { \
        if (!(expr)) { \
            std::ostringstream _oss; \
            _oss << __FILE__ << ":" << __LINE__ << " ASSERT FAILED: " #expr; \
            test::record(test_name, false, _oss.str()); \
            return; \
        } \
    } while(0)

#define TEST_ASSERT_EQ(a, b) \
    do { \
        auto _a = (a); auto _b = (b); \
        if (_a != _b) { \
            std::ostringstream _oss; \
            _oss << __FILE__ << ":" << __LINE__ << " ASSERT_EQ FAILED: " #a " != " #b "\n  got:      " << _a << "\n  expected: " << _b; \
            test::record(test_name, false, _oss.str()); \
            return; \
        } \
    } while(0)

#define TEST_ASSERT_NE(a, b) \
    do { \
        auto _a = (a); auto _b = (b); \
        if (_a == _b) { \
            std::ostringstream _oss; \
            _oss << __FILE__ << ":" << __LINE__ << " ASSERT_NE FAILED: " #a " == " #b " = " << _a; \
            test::record(test_name, false, _oss.str()); \
            return; \
        } \
    } while(0)

#define TEST_ASSERT_LT(a, b) \
    do { \
        auto _a = (a); auto _b = (b); \
        if (!(_a < _b)) { \
            std::ostringstream _oss; \
            _oss << __FILE__ << ":" << __LINE__ << " ASSERT_LT FAILED: " #a " >= " #b " (" << _a << " >= " << _b << ")"; \
            test::record(test_name, false, _oss.str()); \
            return; \
        } \
    } while(0)

#define TEST_ASSERT_GT(a, b) \
    do { \
        auto _a = (a); auto _b = (b); \
        if (!(_a > _b)) { \
            std::ostringstream _oss; \
            _oss << __FILE__ << ":" << __LINE__ << " ASSERT_GT FAILED: " #a " <= " #b " (" << _a << " <= " << _b << ")"; \
            test::record(test_name, false, _oss.str()); \
            return; \
        } \
    } while(0)

#define TEST_ASSERT_LE(a, b) \
    do { \
        auto _a = (a); auto _b = (b); \
        if (!(_a <= _b)) { \
            std::ostringstream _oss; \
            _oss << __FILE__ << ":" << __LINE__ << " ASSERT_LE FAILED: " #a " > " #b; \
            test::record(test_name, false, _oss.str()); \
            return; \
        } \
    } while(0)

#define TEST_ASSERT_GE(a, b) \
    do { \
        auto _a = (a); auto _b = (b); \
        if (!(_a >= _b)) { \
            std::ostringstream _oss; \
            _oss << __FILE__ << ":" << __LINE__ << " ASSERT_GE FAILED: " #a " < " #b; \
            test::record(test_name, false, _oss.str()); \
            return; \
        } \
    } while(0)

#define TEST_ASSERT_THROWS(expr, exc_type) \
    do { \
        bool _threw = false; \
        try { expr; } catch (const exc_type&) { _threw = true; } catch (...) {} \
        if (!_threw) { \
            std::ostringstream _oss; \
            _oss << __FILE__ << ":" << __LINE__ << " ASSERT_THROWS FAILED: " #expr " did not throw " #exc_type; \
            test::record(test_name, false, _oss.str()); \
            return; \
        } \
    } while(0)

#define RUN_TEST(fn) \
    do { \
        std::string test_name = #fn; \
        size_t _before = test::results().size(); \
        fn(test_name); \
        if (test::results().size() == _before) { \
            test::record(test_name, true); \
        } \
    } while(0)

// Convenience: register a test (auto-run in main)
#define DEFINE_TEST(fn) \
    void fn(const std::string& test_name)

inline int run_all() {
    std::cout << "\n========================================\n";
    std::cout << "  Test Results: " << passed_tests() << " passed, "
              << failed_tests() << " failed, " << total_tests() << " total\n";
    std::cout << "========================================\n";

    if (failed_tests() > 0) {
        std::cout << "\nFAILED tests:\n";
        for (auto& r : results()) {
            if (!r.passed) {
                std::cout << "  ✗ " << r.name << "\n    " << r.message << "\n";
            }
        }
    }

    std::cout << "\nPASSED tests:\n";
    for (auto& r : results()) {
        if (r.passed) {
            std::cout << "  ✓ " << r.name << "\n";
        }
    }

    std::cout << std::endl;
    return failed_tests() > 0 ? 1 : 0;
}

} // namespace test
