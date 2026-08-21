import { test, expect, ElectronApplication, Page } from '@playwright/test';
import { _electron as electron } from '@playwright/test';
import AdmZip from 'adm-zip';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

let electronApp: ElectronApplication;
let page: Page;
let tmpDir: string;
let folderFixturePath: string;

type FixtureCase = {
  filePath: string;
  label: string;
  expectedPages: number;
};

test.describe('Knowledge ingest workflow', () => {
  const liveRoot = process.env.BIOROUTER_E2E_PATH_ROOT;
  test.skip(
    process.env.BIOROUTER_E2E_LIVE !== '1' || !liveRoot,
    'Set BIOROUTER_E2E_LIVE=1 and BIOROUTER_E2E_PATH_ROOT to a seeded isolated config.'
  );

  test.beforeAll(async () => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'biorouter-knowledge-ingest-'));
    folderFixturePath = writeFixtures(tmpDir);

    electronApp = await electron.launch({
      args: [path.join(__dirname, '../../.vite/build/main.js')],
      cwd: path.join(__dirname, '../..'),
      env: {
        ...process.env,
        ELECTRON_IS_DEV: '1',
        NODE_ENV: 'development',
        BIOROUTER_ALLOWLIST_BYPASS: 'true',
        BIOROUTER_KNOWLEDGE_TEST_MODE: '1',
        BIOROUTER_PATH_ROOT: liveRoot,
        PLAYWRIGHT_SELECT_PATH: folderFixturePath,
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
    await expect(
      page.getByText('No pages yet. Ingest a source to populate the graph.')
    ).toHaveCount(0);
  });

  test('stages folders and archives through the desktop flow and still forms the graph', async () => {
    await page.getByTestId('sidebar-knowledge-button').click();
    await expect(page.getByRole('heading', { name: 'Knowledge' })).toBeVisible();

    const kbName = `Playwright Path KB ${Date.now()}`;
    await page.getByTestId('knowledge-kb-selector-trigger').click();
    await page.getByTestId('knowledge-kb-create').click();
    await page.getByTestId('knowledge-kb-name-input').fill(kbName);
    await page.getByTestId('knowledge-kb-submit').click();
    await expect(page.getByTestId('knowledge-kb-selector-trigger')).toContainText(kbName);
    await page.keyboard.press('Escape');

    const digestButton = page.getByTestId('knowledge-digest-button');
    const graphSummary = page.getByTestId('knowledge-graph-summary');

    await page.getByText('Drag and drop to stage').click();
    await page.getByTestId('knowledge-ingest-browse-path').click();
    await expect(
      page.locator('[data-testid="knowledge-staged-item"][data-label="folder-input/alpha.md"]')
    ).toBeVisible();
    await expect(
      page.locator('[data-testid="knowledge-staged-item"][data-label="folder-input/beta.txt"]')
    ).toBeVisible();

    await digestButton.click();
    await expect(digestButton).toHaveText('Digest Staged Sources', {
      timeout: 90000,
    });
    await expect(graphSummary).toContainText('2 pages', { timeout: 90000 });

    await page
      .locator('[data-testid="knowledge-ingest-file-input"]')
      .setInputFiles(path.join(tmpDir, 'bundle.zip'));

    await expect(
      page.locator('[data-testid="knowledge-staged-item"][data-label="bundle/gamma.md"]')
    ).toBeVisible();
    await expect(
      page.locator('[data-testid="knowledge-staged-item"][data-label="bundle/delta.csv"]')
    ).toBeVisible();

    await digestButton.click();
    await expect(digestButton).toHaveText('Digest Staged Sources', {
      timeout: 90000,
    });
    await expect(graphSummary).toContainText('4 pages', { timeout: 90000 });
    await expect(page.getByTestId('knowledge-graph-canvas')).toBeVisible();
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

function writeFixtures(baseDir: string): string {
  fs.writeFileSync(
    path.join(baseDir, 'note.md'),
    '# Markdown Fixture\n\nDigest this markdown note.'
  );
  fs.writeFileSync(path.join(baseDir, 'note.txt'), 'Plain text fixture for ingestion.');
  fs.writeFileSync(path.join(baseDir, 'table.csv'), 'name,score\nAlice,9\nBob,7\n');
  fs.writeFileSync(
    path.join(baseDir, 'article.html'),
    fs.readFileSync(
      path.join(
        __dirname,
        '../../../../crates/biorouter-mcp/src/knowledge/convert/fixtures/article.html'
      )
    )
  );
  fs.writeFileSync(
    path.join(baseDir, 'sample.pdf'),
    fs.readFileSync(
      path.join(
        __dirname,
        '../../../../crates/biorouter-mcp/src/computercontroller/tests/data/test.pdf'
      )
    )
  );
  fs.writeFileSync(
    path.join(baseDir, 'sample.docx'),
    fs.readFileSync(
      path.join(
        __dirname,
        '../../../../crates/biorouter-mcp/src/computercontroller/tests/data/sample.docx'
      )
    )
  );

  const folderInput = path.join(baseDir, 'folder-input');
  fs.mkdirSync(folderInput, { recursive: true });
  fs.writeFileSync(path.join(folderInput, 'alpha.md'), '# Alpha\n\nFolder fixture markdown.');
  fs.writeFileSync(path.join(folderInput, 'beta.txt'), 'Folder fixture plain text.');
  fs.writeFileSync(path.join(folderInput, 'ignore.exe'), Buffer.from([0, 1, 2, 3]));

  const zip = new AdmZip();
  zip.addFile('docs/gamma.md', Buffer.from('# Gamma\n\nArchive fixture markdown.'));
  zip.addFile('docs/delta.csv', Buffer.from('name,score\nGamma,8\nDelta,7\n'));
  zip.addFile('__MACOSX/docs/._gamma.md', Buffer.from('metadata'));
  zip.writeZip(path.join(baseDir, 'bundle.zip'));

  return folderInput;
}
