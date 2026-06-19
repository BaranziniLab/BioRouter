#include "test_framework.hpp"
#include "dp/matrix_chain.hpp"

TEST_CASE("Matrix Chain — classic example") {
    // A1(10×30), A2(30×5), A3(5×60) → dims={10,30,5,60}
    // Best: (A1*A2)*A3 = 10*30*5 + 10*5*60 = 1500+3000 = 4500
    auto r = dp::matrix_chain({10, 30, 5, 60});
    REQUIRE_EQ(r.value, 4500LL);
}

TEST_CASE("Matrix Chain — single matrix") {
    auto r = dp::matrix_chain({10, 20});
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Matrix Chain — two matrices") {
    // A1(10×20), A2(20×5) → cost = 10*20*5 = 1000
    auto r = dp::matrix_chain({10, 20, 5});
    REQUIRE_EQ(r.value, 1000LL);
}

TEST_CASE("Matrix Chain — four matrices") {
    // dims={40,20,30,10,30}
    // A1(40×20), A2(20×30), A3(30×10), A4(10×30)
    // Optimal: A1*(A2*(A3*A4)) = 30*10*30 + 20*30*30 + 40*20*30 = 9000+18000+24000 = 51000? Let me check other...
    // (A1*A2)*(A3*A4) = 40*20*30 + 30*10*30 + 40*30*30 = 24000+9000+36000 = 69000
    // A1*((A2*A3)*A4) = 20*30*10 + 20*10*30 + 40*20*30 = 6000+6000+24000 = 36000
    // (A1*(A2*A3))*A4 = 20*30*10 + 40*20*10 + 40*10*30 = 6000+8000+12000 = 26000
    auto r = dp::matrix_chain({40, 20, 30, 10, 30});
    REQUIRE_EQ(r.value, 26000LL);
}

TEST_CASE("Matrix Chain — empty dims") {
    auto r = dp::matrix_chain({});
    REQUIRE_EQ(r.value, 0LL);
}
