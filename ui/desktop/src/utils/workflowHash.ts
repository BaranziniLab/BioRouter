// THIS MODULE OWNS WORKFLOW HASHES AND NOTHING ELSE.
//
// It used to also register a second `ipcMain.on('close-window')` listener that
// closed `BrowserWindow.getFocusedWindow()`. `ipcMain.on` is additive, so ONE
// `close-window` message ran both that handler and the real one in main.ts —
// and the two disagreed about which window they meant. The real one closes the
// SENDER; this one closed whatever happened to be focused.
//
// For years that was invisible, because the window asking to close is normally
// the focused one, so both handlers closed the same window. The tab-merge path
// is the first caller where they differ: main focuses the TARGET window
// (`target.show(); target.focus(); target.moveTop()`) immediately before the
// SOURCE renderer asks to close itself (D6a). The focused window is then the
// target, so this handler closed the target and the real one closed the source
// — a merge drop took BOTH windows down, and merging the last two left the app
// with zero windows and no way back except relaunching.
//
// Do not add window-lifecycle IPC here. `close-window` has exactly one owner,
// in main.ts, and it is scoped to `event.sender`.
import { ipcMain, app } from 'electron';
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'crypto';

function calculateWorkflowHash(workflow: unknown): string {
  const hash = crypto.createHash('sha256');
  hash.update(JSON.stringify(workflow));
  return hash.digest('hex');
}

async function getWorkflowHashesDir(): Promise<string> {
  const userDataPath = app.getPath('userData');
  const hashesDir = path.join(userDataPath, 'workflow_hashes');
  await fs.mkdir(hashesDir, { recursive: true });
  return hashesDir;
}

ipcMain.handle('has-accepted-workflow-before', async (_event, workflow) => {
  const hash = calculateWorkflowHash(workflow);
  const hashFile = path.join(await getWorkflowHashesDir(), `${hash}.hash`);
  try {
    await fs.access(hashFile);
    return true;
  } catch (err) {
    if (typeof err === 'object' && err !== null && 'code' in err && err.code === 'ENOENT') {
      return false;
    }
    throw err;
  }
});

ipcMain.handle('record-workflow-hash', async (_event, workflow) => {
  const hash = calculateWorkflowHash(workflow);
  const filePath = path.join(await getWorkflowHashesDir(), `${hash}.hash`);
  const timestamp = new Date().toISOString();
  await fs.writeFile(filePath, timestamp);
  return true;
});
