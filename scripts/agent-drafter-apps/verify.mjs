// Drive a browser-opened BioRouter app with Playwright and verify the live
// agent answers. Run from the main repo's ui/desktop dir so `playwright`
// resolves:  node <thisfile> <url> "<prompt>"
import { chromium } from 'playwright';

const url = process.argv[2];
const prompt = process.argv[3] || 'Say hello in one short sentence.';
const tag = process.argv[4] || 'app';

if (!url) {
  console.error('usage: node verify.mjs <url> "<prompt>" [tag]');
  process.exit(2);
}

const errors = [];
const browser = await chromium.launch();
const page = await browser.newPage();
page.on('console', (m) => {
  if (m.type() === 'error') errors.push(m.text());
});
page.on('pageerror', (e) => errors.push(String(e)));

try {
  await page.goto(url, { waitUntil: 'networkidle', timeout: 30000 });
  await page.waitForTimeout(500);
  await page.screenshot({ path: `/tmp/${tag}-loaded.png` });

  // Type into the chat input and send.
  const input = page.locator('.br-input').first();
  await input.waitFor({ timeout: 10000 });
  await input.fill(prompt);
  await page.locator('.br-btn').first().click();

  // Wait for a non-empty agent reply to stream in.
  await page.waitForFunction(
    () => {
      const els = document.querySelectorAll('.br-msg--agent');
      const last = els[els.length - 1];
      return last && last.textContent && last.textContent.trim().length > 3;
    },
    { timeout: 90000 }
  );
  // Allow streaming to settle.
  await page.waitForTimeout(2000);

  const reply = await page.evaluate(() => {
    const els = document.querySelectorAll('.br-msg--agent');
    return els[els.length - 1].textContent;
  });
  await page.screenshot({ path: `/tmp/${tag}-reply.png` });

  console.log(`RESULT ${tag} OK`);
  console.log('AGENT_REPLY:', JSON.stringify(reply).slice(0, 600));
  console.log('CONSOLE_ERRORS:', JSON.stringify(errors));
  await browser.close();
  process.exit(0);
} catch (e) {
  await page.screenshot({ path: `/tmp/${tag}-error.png` }).catch(() => {});
  console.log(`RESULT ${tag} FAIL: ${e}`);
  console.log('CONSOLE_ERRORS:', JSON.stringify(errors));
  await browser.close();
  process.exit(1);
}
