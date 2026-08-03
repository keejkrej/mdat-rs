import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const BIN = resolve(here, "../../../..", "target/debug/mdat-view");
import { chromium } from "playwright";
import { writeMdatTree, W, H } from "./fixtures.mjs";

const FIXTURE_ROOT = mkdtempSync(join(tmpdir(), "mdat-smoke-"));
const TIMEOUT_MS = 30000;

async function bootServer(fixturePath) {
  const child = spawn(BIN, [fixturePath, "--no-open"], { stdio: ["ignore", "pipe", "pipe"] });
  let urlLine = "";
  const urlPromise = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("server did not print URL in time")), TIMEOUT_MS);
    child.stdout.on("data", (chunk) => {
      urlLine += chunk.toString();
      const m = urlLine.match(/http:\/\/\d+\.\d+\.\d+\.\d+:\d+\/[A-Za-z0-9_-]{16,}\//);
      if (m) {
        clearTimeout(timer);
        resolve(m[0]);
      }
    });
    child.stderr.on("data", (chunk) => {
      process.stderr.write(`[mdat-view stderr] ${chunk}`);
    });
    child.on("exit", (code) => {
      if (code !== null) reject(new Error(`server exited early with code ${code}`));
    });
  });
  const url = await urlPromise;
  return { child, url };
}

async function waitForDataset(page) {
  await page.waitForSelector("h1", { timeout: TIMEOUT_MS });
  await page.waitForFunction(() => {
    const h = document.querySelector("h1");
    return h && h.textContent && h.textContent !== "mdat view";
  }, { timeout: TIMEOUT_MS });
}

async function waitForPlaneCount(requests, min) {
  const start = Date.now();
  while (Date.now() - start < TIMEOUT_MS) {
    if (requests.filter((u) => u.includes("/plane")).length >= min) return;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`timed out waiting for >=${min} plane fetches; got ${requests.filter((u) => u.includes("/plane")).length}`);
}

async function run() {
  const fixture = writeMdatTree(join(FIXTURE_ROOT, "multi"), 2, 3, 2, 1);
  const { child, url } = await bootServer(fixture);
  let exitCode = 0;
  let browser;
  try {
    const execPath = "/usr/bin/chromium";
    browser = await chromium.launch({
      headless: true,
      executablePath: execPath,
      args: ["--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage"],
    });
    const page = await browser.newPage();
    const requests = [];
    page.on("request", (req) => {
      const u = req.url();
      if (u.includes("/dataset") || u.includes("/plane")) requests.push(u);
    });

    await page.goto(url, { waitUntil: "networkidle", timeout: TIMEOUT_MS });
    await waitForDataset(page);

    await page.waitForSelector("canvas", { timeout: TIMEOUT_MS });
    await waitForPlaneCount(requests, 1);

    const toggle = page.locator('button[title="toggle channel 1"]');
    await toggle.click();
    await page.waitForTimeout(300);

    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(300);

    await page.keyboard.press("a");
    await page.waitForTimeout(200);

    const posSelect = page.locator("select").first();
    await posSelect.selectOption("1");
    await page.waitForTimeout(400);

    const dims = await page.locator("header span").first().textContent();
    console.log("dataset header dims:", dims?.trim());

    const planeCount = requests.filter((u) => u.includes("/plane")).length;
    if (planeCount < 3) throw new Error(`expected >=3 plane fetches, got ${planeCount}`);

    const datasetCount = requests.filter((u) => u.includes("/dataset")).length;
    if (datasetCount < 1) throw new Error("expected at least 1 dataset fetch");

    const tAfterScrub = await page.locator("span.tabular-nums").first().textContent();
    console.log("time nav after scrub:", tAfterScrub?.trim());

    console.log(`PASS: smoke completed with ${requests.length} API requests, ${planeCount} plane fetches, ${datasetCount} dataset fetches`);
  } catch (err) {
    console.error("FAIL:", err.message);
    exitCode = 1;
  } finally {
    if (browser) await browser.close();
    child.kill("SIGINT");
    await new Promise((r) => child.on("exit", r));
  }
  rmSync(FIXTURE_ROOT, { recursive: true, force: true });
  process.exit(exitCode);
}

run().catch((err) => {
  console.error("FATAL:", err);
  rmSync(FIXTURE_ROOT, { recursive: true, force: true });
  process.exit(1);
});