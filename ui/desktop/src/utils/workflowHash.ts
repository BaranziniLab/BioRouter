import { ipcMain, app, BrowserWindow } from 'electron';
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

ipcMain.on('close-window', () => {
  const currentWindow = BrowserWindow.getFocusedWindow();
  if (currentWindow) {
    currentWindow.close();
  }
});
