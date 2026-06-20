test_that("predictions on response scale match glm()", {
  dat = simulate_gaussian_data(n = 200, seed = 50)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "gaussian")
  ref = glm(y ~ x1 + x2, data = dat, family = stats::gaussian())

  pred_fit = predict(fit, type = "response")
  pred_ref = predict(ref, type = "response")

  expect_equal(pred_fit, pred_ref, tolerance = 1e-5)
})

test_that("predictions on link scale match glm()", {
  dat = simulate_gaussian_data(n = 200, seed = 51)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "gaussian")
  ref = glm(y ~ x1 + x2, data = dat, family = stats::gaussian())

  pred_fit = predict(fit, type = "link")
  pred_ref = predict(ref, type = "link")

  expect_equal(pred_fit, pred_ref, tolerance = 1e-5)
})

test_that("predictions with newdata work", {
  dat = simulate_gaussian_data(n = 200, seed = 52)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "gaussian")

  newdata = data.frame(x1 = c(0, 1, -1), x2 = c(0.5, -0.5, 0))
  pred = predict(fit, newdata = newdata, type = "response")

  expect_length(pred, 3)
  expect_true(all(is.finite(pred)))
})

test_that("prediction CIs have correct width", {
  dat = simulate_gaussian_data(n = 200, seed = 53)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "gaussian")

  pred = predict(fit, type = "response", ci = TRUE, ci.level = 0.95)

  expect_true("lwr" %in% names(pred))
  expect_true("upr" %in% names(pred))
  expect_true(all(pred$lwr <= pred$fit))
  expect_true(all(pred$upr >= pred$fit))
})

test_that("prediction SE matches glm() approximately", {
  dat = simulate_gaussian_data(n = 300, seed = 54)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "gaussian")
  ref = glm(y ~ x1 + x2, data = dat, family = stats::gaussian())

  se_fit = predict(fit, type = "link", se.fit = TRUE)$se.fit
  se_ref = predict(ref, type = "link", se.fit = TRUE)$se.fit

  expect_equal(se_fit, se_ref, tolerance = 1e-4)
})

test_that("binomial predictions on response scale are probabilities", {
  dat = simulate_binomial_data(n = 200, seed = 55)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "binomial")

  pred = predict(fit, type = "response")
  expect_true(all(pred >= 0 & pred <= 1))
})
