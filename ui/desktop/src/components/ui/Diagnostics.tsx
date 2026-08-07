import React, { useState } from 'react';
import { AlertTriangle, Download, Github, Loader2 } from '../icons/app-icons';
import { Button } from './button';
import { toastError, toastSuccess } from '../../toasts';
import { diagnostics, systemInfo } from '../../api';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './dialog';

interface DiagnosticsModalProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
}

export const DiagnosticsModal: React.FC<DiagnosticsModalProps> = ({
  isOpen,
  onClose,
  sessionId,
}) => {
  const [isDownloading, setIsDownloading] = useState(false);
  const [isFilingBug, setIsFilingBug] = useState(false);

  const handleDownload = async () => {
    setIsDownloading(true);

    try {
      const response = await diagnostics({
        path: { session_id: sessionId },
        throwOnError: true,
      });

      const archive = await response.data.arrayBuffer();
      const result = await window.electron.saveDiagnosticsBundle(sessionId, archive);

      if (result.error) {
        throw new Error(result.error);
      }
      if (result.canceled) {
        return;
      }

      toastSuccess({
        title: 'Diagnostics saved',
        msg: result.filePath
          ? `The diagnostics bundle was saved to ${result.filePath}.`
          : 'The diagnostics bundle was saved successfully.',
      });
      onClose();
    } catch (error) {
      toastError({
        title: 'Diagnostics Error',
        msg: error instanceof Error ? error.message : 'Failed to generate diagnostics.',
      });
    } finally {
      setIsDownloading(false);
    }
  };

  const handleFileGitHubIssue = async () => {
    setIsFilingBug(true);

    try {
      const response = await systemInfo({ throwOnError: true });
      const info = response.data;

      const providerModel =
        info.provider && info.model
          ? `${info.provider}: ${info.model}`
          : info.provider || info.model || '[e.g. Google: gemini-1.5-pro]';

      const extensions =
        info.enabled_extensions.length > 0
          ? info.enabled_extensions.join(', ')
          : '[e.g. Computer Controller, Figma]';

      const body = `**Describe the bug**

💡 Before filing, please check common issues:  
https://github.com/BaranziniLab/biorouter/docs/troubleshooting

📦 To help us debug faster, attach your **diagnostics zip** if possible.  
👉 How to capture it: https://github.com/BaranziniLab/biorouter/docs/troubleshooting/diagnostics-and-reporting/

A clear and concise description of what the bug is.

---

**To Reproduce**
Steps to reproduce the behavior:
1. Go to '...'
2. Click on '....'
3. Scroll down to '....'
4. See error

---

**Expected behavior**
A clear and concise description of what you expected to happen.

---

**Screenshots**
If applicable, add screenshots to help explain your problem.

---

**Please provide the following information**
- **OS & Arch:** ${info.os} ${info.os_version} ${info.architecture}
- **Interface:** UI
- **Version:** ${info.app_version}
- **Extensions enabled:** ${extensions}
- **Provider & Model:** ${providerModel}

---

**Additional context**
Add any other context about the problem here.
`;

      const params = new URLSearchParams({
        template: 'bug_report.md',
        body: body,
        labels: 'bug',
      });

      window.open(
        `https://github.com/BaranziniLab/biorouter/issues/new?${params.toString()}`,
        '_blank'
      );
      onClose();
    } catch {
      toastError({
        title: 'Error',
        msg: 'Failed to get system information',
      });
    } finally {
      setIsFilingBug(false);
    }
  };

  const isBusy = isDownloading || isFilingBug;

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && !isBusy && onClose()}>
      <DialogContent dismissible={!isBusy} className="sm:max-w-lg">
        <DialogHeader className="mb-0">
          <div className="flex items-start gap-3">
            <div className="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-container border border-border-subtle bg-background-warning/40 text-text-warning">
              <AlertTriangle size={20} />
            </div>
            <div className="min-w-0 flex-1">
              <DialogTitle>Report a Problem</DialogTitle>
              <DialogDescription className="mt-2">
                You can download a diagnostics zip file to share with the team, or file a bug
                directly on GitHub with your system details pre-filled. A diagnostics report
                contains the following:
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>
        <div className="space-y-3 text-body text-text-muted">
          <ul className="list-inside list-disc space-y-1">
            <li>Basic system info</li>
            <li>Your current session messages</li>
            <li>Recent log files</li>
            <li>Configuration settings</li>
          </ul>
          <p>
            <strong className="text-text-default">Warning:</strong> If your session contains
            sensitive information, do not share the diagnostics file publicly.
          </p>
          <p>If you file a bug, consider attaching the diagnostics report to it.</p>
        </div>
        {isDownloading && (
          <div
            className="flex items-center gap-3 rounded-container border border-border-subtle bg-background-muted px-3 py-2.5 text-body text-text-muted"
            role="status"
          >
            <Loader2 className="h-4 w-4 shrink-0 animate-spin" />
            Preparing diagnostics bundle...
          </div>
        )}
        <DialogFooter className="sm:flex-wrap">
          <Button onClick={onClose} variant="outline" size="sm" disabled={isBusy}>
            Cancel
          </Button>
          <Button onClick={handleDownload} variant="outline" size="sm" disabled={isBusy}>
            <Download size={16} className="mr-1" />
            {isDownloading ? 'Generating...' : 'Generate diagnostics'}
          </Button>
          <Button
            onClick={handleFileGitHubIssue}
            variant="outline"
            size="sm"
            disabled={isBusy}
            className="bg-background-accent text-text-on-accent hover:bg-background-accent/90"
          >
            <Github size={16} className="mr-1" />
            {isFilingBug ? 'Opening...' : 'File Bug on GitHub'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
