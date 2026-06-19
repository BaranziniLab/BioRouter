#pragma once
// Minimal Catch2-inspired assertion test framework (single-header, no dependencies).
#include <iostream>
#include <string>
#include <vector>
#include <cmath>
#include <functional>
#include <sstream>
#include <stdexcept>

namespace ttf {

struct TestCase {
    std::string name;
    std::function<void()> fn;
};

inline std::vector<TestCase>& registry() {
    static std::vector<TestCase> cases;
    return cases;
}

inline int add_test(const std::string& name, std::function<void()> fn) {
    registry().push_back({name, std::move(fn)});
    return 0;
}

struct AssertionFailure : std::runtime_error {
    using std::runtime_error::runtime_error;
};

inline void report_fail(const char* expr, const char* file, int line, const std::string& extra = "") {
    std::ostringstream os;
    os << "FAILED: " << expr << "  at " << file << ":" << line;
    if (!extra.empty()) os << "\n  " << extra;
    throw AssertionFailure(os.str());
}

} // namespace ttf

#define TEST_CASE(Name)                                                         \
    static void _ttf_fn_##__LINE__();                                           \
    static int _ttf_reg_##__LINE__ = ::ttf::add_test(Name, _ttf_fn_##__LINE__); \
    static void _ttf_fn_##__LINE__()

// Use __COUNTER__ for uniqueness; fall back to __LINE__ if needed
#define _TTB_CONCAT(a, b) a##b
#define _TTB_ID(a, b) _TTB_CONCAT(a, b)

#undef TEST_CASE
#define TEST_CASE(Name)                                                           \
    static void _TTB_ID(_ttf_fn_, __LINE__)();                                    \
    static int _TTB_ID(_ttf_reg_, __LINE__) =                                     \
        ::ttf::add_test(Name, _TTB_ID(_ttf_fn_, __LINE__));                       \
    static void _TTB_ID(_ttf_fn_, __LINE__)()

#define REQUIRE(expr)                                                             \
    do {                                                                          \
        if (!(expr))                                                              \
            ::ttf::report_fail(#expr, __FILE__, __LINE__);                        \
    } while (0)

#define REQUIRE_EQ(a, b)                                                          \
    do {                                                                          \
        if (!((a) == (b))) {                                                      \
            std::ostringstream _ttb_os;                                           \
            _ttb_os << "  actual: " << (a) << "\n  expected: " << (b);            \
            ::ttf::report_fail(#a " == " #b, __FILE__, __LINE__, _ttb_os.str());  \
        }                                                                         \
    } while (0)

#define REQUIRE_CLOSE(a, b, eps)                                                  \
    do {                                                                          \
        if (std::abs((double)(a) - (double)(b)) > (eps)) {                        \
            std::ostringstream _ttb_os;                                           \
            _ttb_os << "  actual: " << (a) << "\n  expected: " << (b)             \
                     << "\n  epsilon: " << (eps);                                 \
            ::ttf::report_fail("|" #a " - " #b "| <= " #eps, __FILE__, __LINE__, _ttb_os.str()); \
        }                                                                         \
    } while (0)

#define REQUIRE_THROWS(expr)                                                      \
    do {                                                                          \
        bool _ttb_threw = false;                                                  \
        try { expr; } catch (...) { _ttb_threw = true; }                          \
        if (!_ttb_threw)                                                          \
            ::ttf::report_fail(#expr " should throw", __FILE__, __LINE__);        \
    } while (0)

inline int run_all_tests() {
    int pass = 0, fail = 0;
    for (auto& tc : ttf::registry()) {
        try {
            tc.fn();
            std::cout << "  PASS  " << tc.name << "\n";
            ++pass;
        } catch (const ttf::AssertionFailure& e) {
            std::cout << "  FAIL  " << tc.name << "\n    " << e.what() << "\n";
            ++fail;
        } catch (const std::exception& e) {
            std::cout << "  ERROR " << tc.name << "\n    " << e.what() << "\n";
            ++fail;
        }
    }
    std::cout << "\n=== Results: " << pass << " passed, " << fail << " failed, "
              << pass + fail << " total ===\n";
    return fail > 0 ? 1 : 0;
}
