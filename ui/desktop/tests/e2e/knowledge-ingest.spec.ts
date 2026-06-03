import { test, expect, ElectronApplication, Page } from '@playwright/test';
import { _electron as electron } from '@playwright/test';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

let electronApp: ElectronApplication;
let page: Page;
let tmpDir: string;

type FixtureCase = {
  filePath: string;
  label: string;
  expectedPages: number;
};

test.describe('Knowledge ingest workflow', () => {
  test.beforeAll(async () => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'biorouter-knowledge-ingest-'));
    writeFixtures(tmpDir);

    electronApp = await electron.launch({
      args: [path.join(__dirname, '../../.vite/build/main.js')],
      cwd: path.join(__dirname, '../..'),
      env: {
        ...process.env,
        ELECTRON_IS_DEV: '1',
        NODE_ENV: 'development',
        BIOROUTER_ALLOWLIST_BYPASS: 'true',
        BIOROUTER_KNOWLEDGE_TEST_MODE: '1',
        ELECTRON_RUN_AS_NODE: '',
      },
    });

    page = await electronApp.firstWindow();
    await page.waitForLoadState('domcontentloaded');
    await page.waitForFunction(() => {
      const root = document.getElementById('root');
      return root && root.children.length > 0;
    });
    await page.waitForTimeout(1500);
  });

  test.afterAll(async () => {
    if (electronApp) {
      await electronApp.close().catch(() => {});
    }
    if (tmpDir && fs.existsSync(tmpDir)) {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  test('digests supported files and updates the graph without errors', async () => {
    await page.getByTestId('sidebar-knowledge-button').click();
    await expect(page.getByRole('heading', { name: 'Knowledge' })).toBeVisible();

    const kbName = `Playwright KB ${Date.now()}`;
    await page.getByTestId('knowledge-kb-selector-trigger').click();
    await page.getByTestId('knowledge-kb-create').click();
    await page.getByTestId('knowledge-kb-name-input').fill(kbName);
    await page.getByTestId('knowledge-kb-submit').click();
    await expect(page.getByTestId('knowledge-kb-selector-trigger')).toContainText(kbName);
    await page.keyboard.press('Escape');

    const digestButton = page.getByTestId('knowledge-digest-button');
    const graphSummary = page.getByTestId('knowledge-graph-summary');

    for (const fixture of fixtureCases(tmpDir)) {
      await page
        .locator('[data-testid="knowledge-ingest-file-input"]')
        .setInputFiles(fixture.filePath);

      const stagedItem = page.locator(
        `[data-testid="knowledge-staged-item"][data-label="${fixture.label}"]`
      );
      await expect(stagedItem).toBeVisible();
      await expect(digestButton).toBeEnabled();

      await digestButton.click();
      await expect(digestButton).toHaveText(/Checking model…|Digesting…|Stopping…/);
      await expect(digestButton).toHaveText('Digest Staged Sources', {
        timeout: 90000,
      });
      await expect(stagedItem).toHaveCount(0, { timeout: 90000 });

      await expect(graphSummary).toContainText(
        `${fixture.expectedPages} ${fixture.expectedPages === 1 ? 'page' : 'pages'}`,
        { timeout: 90000 }
      );
    }

    await expect(page.getByTestId('knowledge-graph-canvas')).toBeVisible();
    await expect(page.getByText('No pages yet. Ingest a source to populate the graph.')).toHaveCount(0);
  });
});

function fixtureCases(baseDir: string): FixtureCase[] {
  return [
    { filePath: path.join(baseDir, 'note.md'), label: 'note.md', expectedPages: 1 },
    { filePath: path.join(baseDir, 'note.txt'), label: 'note.txt', expectedPages: 2 },
    { filePath: path.join(baseDir, 'table.csv'), label: 'table.csv', expectedPages: 3 },
    { filePath: path.join(baseDir, 'article.html'), label: 'article.html', expectedPages: 4 },
    { filePath: path.join(baseDir, 'sample.pdf'), label: 'sample.pdf', expectedPages: 5 },
    { filePath: path.join(baseDir, 'sample.docx'), label: 'sample.docx', expectedPages: 6 },
  ];
}

function writeFixtures(baseDir: string): void {
  fs.writeFileSync(path.join(baseDir, 'note.md'), '# Markdown Fixture\n\nDigest this markdown note.');
  fs.writeFileSync(path.join(baseDir, 'note.txt'), 'Plain text fixture for ingestion.');
  fs.writeFileSync(path.join(baseDir, 'table.csv'), 'name,score\nAlice,9\nBob,7\n');
  fs.writeFileSync(
    path.join(baseDir, 'article.html'),
    fs.readFileSync(
      path.join(__dirname, '../../../../crates/biorouter-mcp/src/knowledge/convert/fixtures/article.html')
    )
  );
  fs.writeFileSync(
    path.join(baseDir, 'sample.pdf'),
    fs.readFileSync(
      path.join(__dirname, '../../../../crates/biorouter-mcp/src/computercontroller/tests/data/test.pdf')
    )
  );
  fs.writeFileSync(
    path.join(baseDir, 'sample.docx'),
    fs.readFileSync(
      path.join(__dirname, '../../../../crates/biorouter-mcp/src/computercontroller/tests/data/sample.docx')
    )
  );
}
