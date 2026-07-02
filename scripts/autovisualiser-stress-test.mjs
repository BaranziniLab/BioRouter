#!/usr/bin/env node

import { createRequire } from "node:module";
import { mkdir, readdir, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { spawn } from "node:child_process";
import process from "node:process";

const repoRoot = resolve(new URL("..", import.meta.url).pathname);
const galleryDir = process.env.AUTOVIS_GALLERY_DIR || "/tmp/av_gallery";
const screenshotDir =
  process.env.AUTOVIS_SCREENSHOT_DIR || "/tmp/av_gallery_screenshots";
const reportPath = process.env.AUTOVIS_REPORT || "/tmp/av_gallery_report.json";

const viewports = [
  { name: "desktop", width: 1280, height: 900 },
  { name: "compact", width: 430, height: 760 },
];

function run(command, args, options = {}) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, {
      cwd: repoRoot,
      env: process.env,
      stdio: "inherit",
      ...options,
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) {
        resolveRun();
      } else {
        reject(new Error(`${command} ${args.join(" ")} exited with ${code}`));
      }
    });
  });
}

async function loadPlaywright() {
  const requireFromDesktop = createRequire(
    new URL("../ui/desktop/package.json", import.meta.url),
  );
  try {
    return requireFromDesktop("playwright");
  } catch (error) {
    throw new Error(
      `Could not load Playwright from ui/desktop/node_modules. Run npm install in ui/desktop first. ${error.message}`,
    );
  }
}

async function launchChromium(chromium) {
  try {
    return await chromium.launch();
  } catch (error) {
    const candidates = [
      process.env.AUTOVIS_BROWSER,
      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      "/Applications/Chromium.app/Contents/MacOS/Chromium",
      "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
      "/usr/bin/google-chrome",
      "/usr/bin/chromium",
      "/usr/bin/chromium-browser",
    ].filter(Boolean);

    for (const executablePath of candidates) {
      if (!existsSync(executablePath)) continue;
      console.log(`Playwright browser missing; using ${executablePath}`);
      return chromium.launch({ executablePath });
    }

    throw error;
  }
}

async function generateGallery() {
  await rm(galleryDir, { recursive: true, force: true });
  await mkdir(galleryDir, { recursive: true });
  await run("cargo", [
    "test",
    "-p",
    "biorouter-mcp",
    "--lib",
    "autovisualiser::tests::generate_gallery",
    "--",
    "--ignored",
  ]);
}

function isIgnorableConsoleError(text) {
  return /favicon\.ico|ERR_ABORTED|ERR_BLOCKED_BY_CLIENT/.test(text);
}

async function inspectPage(page) {
  return page.evaluate(() => {
    function hasVisibleBox(element) {
      var rect = element.getBoundingClientRect();
      return rect.width >= 80 && rect.height >= 80;
    }

    function canvasHasInk(canvas) {
      var rect = canvas.getBoundingClientRect();
      if (
        rect.width < 80 ||
        rect.height < 80 ||
        !canvas.width ||
        !canvas.height
      )
        return false;
      try {
        var ctx = canvas.getContext("2d");
        var data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
        var step = Math.max(4, Math.floor(data.length / 5000));
        var ink = 0;
        for (var i = 0; i < data.length; i += step - (step % 4)) {
          var r = data[i];
          var g = data[i + 1];
          var b = data[i + 2];
          var a = data[i + 3];
          if (a > 8 && !(r > 245 && g > 245 && b > 245)) ink += 1;
          if (ink > 24) return true;
        }
      } catch (error) {
        return hasVisibleBox(canvas);
      }
      return false;
    }

    var canvases = Array.from(document.querySelectorAll("canvas"));
    var svgs = Array.from(document.querySelectorAll("svg"));
    var leaflet = Array.from(document.querySelectorAll(".leaflet-container"));
    var visualCanvases = canvases.filter(canvasHasInk).length;
    var visualSvgs = svgs.filter(function (svg) {
      return (
        hasVisibleBox(svg) &&
        svg.querySelectorAll("path,circle,line,rect,text,polygon,polyline,g")
          .length > 0
      );
    }).length;
    var visualMaps = leaflet.filter(hasVisibleBox).length;
    var errorText =
      /could not be rendered|diagram syntax could not be rendered/i.test(
        document.body ? document.body.innerText : "",
      );
    var bodyText = document.body ? document.body.innerText.trim() : "";

    return {
      title: document.title,
      bodyTextLength: bodyText.length,
      canvasCount: canvases.length,
      visualCanvases: visualCanvases,
      svgCount: svgs.length,
      visualSvgs: visualSvgs,
      mapCount: leaflet.length,
      visualMaps: visualMaps,
      errorText: errorText,
      hasVisual: visualCanvases + visualSvgs + visualMaps > 0,
    };
  });
}

async function renderOne(browser, file, viewport) {
  const page = await browser.newPage({ viewport });
  const consoleErrors = [];
  const pageErrors = [];

  page.on("console", (message) => {
    if (message.type() !== "error") return;
    const text = message.text();
    if (!isIgnorableConsoleError(text)) consoleErrors.push(text);
  });
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  const url = pathToFileURL(join(galleryDir, file)).href;
  const shot = join(
    screenshotDir,
    `${basename(file, ".html")}-${viewport.name}.png`,
  );
  let inspection;
  try {
    await page.goto(url, { waitUntil: "domcontentloaded", timeout: 30000 });
    await page.waitForLoadState("load", { timeout: 30000 }).catch(() => {});
    await page
      .waitForFunction(
        () =>
          document.querySelector("canvas,svg,.leaflet-container") ||
          /could not be rendered/i.test(document.body?.innerText || ""),
        null,
        { timeout: 10000 },
      )
      .catch(() => {});
    await page.waitForTimeout(1200);
    inspection = await inspectPage(page);
    await page.screenshot({ path: shot, fullPage: false });
  } finally {
    await page.close();
  }

  const failures = [];
  if (!inspection.hasVisual) failures.push("no visible canvas/svg/map content");
  if (inspection.errorText) failures.push("rendered visualizer error card");
  if (consoleErrors.length)
    failures.push(`${consoleErrors.length} console error(s)`);
  if (pageErrors.length) failures.push(`${pageErrors.length} page error(s)`);

  return {
    file,
    viewport: viewport.name,
    screenshot: shot,
    inspection,
    consoleErrors,
    pageErrors,
    ok: failures.length === 0,
    failures,
  };
}

async function main() {
  await generateGallery();
  if (!existsSync(galleryDir))
    throw new Error(`Gallery was not written to ${galleryDir}`);

  const files = (await readdir(galleryDir))
    .filter((file) => file.endsWith(".html"))
    .sort();
  if (!files.length)
    throw new Error(`No generated HTML visualizations found in ${galleryDir}`);

  await rm(screenshotDir, { recursive: true, force: true });
  await mkdir(screenshotDir, { recursive: true });

  const { chromium } = await loadPlaywright();
  const browser = await launchChromium(chromium);
  const results = [];
  try {
    for (const file of files) {
      for (const viewport of viewports) {
        process.stdout.write(`Rendering ${file} (${viewport.name}) ... `);
        const result = await renderOne(browser, file, viewport);
        results.push(result);
        process.stdout.write(
          result.ok ? "ok\n" : `FAILED: ${result.failures.join(", ")}\n`,
        );
      }
    }
  } finally {
    await browser.close();
  }

  const failures = results.filter((result) => !result.ok);
  const report = {
    generatedAt: new Date().toISOString(),
    galleryDir,
    screenshotDir,
    viewports,
    totalHtmlFiles: files.length,
    totalChecks: results.length,
    failures: failures.length,
    results,
  };
  await writeFile(reportPath, JSON.stringify(report, null, 2));

  console.log(`\nAuto Visualiser stress report: ${reportPath}`);
  console.log(`Screenshots: ${screenshotDir}`);
  console.log(
    `Checked ${files.length} visualization(s) across ${viewports.length} viewport(s).`,
  );

  if (failures.length) {
    console.error(`\n${failures.length} render check(s) failed:`);
    for (const failure of failures) {
      console.error(
        `- ${failure.file} (${failure.viewport}): ${failure.failures.join(", ")}`,
      );
      for (const error of failure.consoleErrors.slice(0, 3))
        console.error(`  console: ${error}`);
      for (const error of failure.pageErrors.slice(0, 3))
        console.error(`  page: ${error}`);
    }
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
