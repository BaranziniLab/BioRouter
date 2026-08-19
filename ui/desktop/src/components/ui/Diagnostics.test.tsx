import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { diagnostics } from '../../api';
import { toastError, toastSuccess } from '../../toasts';
import { userActionHeaders } from '../../utils/userAction';
import { DiagnosticsModal } from './Diagnostics';

vi.mock('../../api', () => ({
  diagnostics: vi.fn(),
  systemInfo: vi.fn(),
}));

vi.mock('../../toasts', () => ({
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
}));

vi.mock('../../utils/userAction', () => ({
  userActionHeaders: vi.fn(),
}));

const diagnosticsMock = vi.mocked(diagnostics);
const toastErrorMock = vi.mocked(toastError);
const toastSuccessMock = vi.mocked(toastSuccess);
const userActionHeadersMock = vi.mocked(userActionHeaders);
const saveDiagnosticsBundle = vi.fn();

const diagnosticsResponse = () => ({
  data: {
    arrayBuffer: vi.fn().mockResolvedValue(Uint8Array.from([0x50, 0x4b, 0x03, 0x04]).buffer),
  },
});

describe('DiagnosticsModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as unknown as { electron: unknown }).electron = { saveDiagnosticsBundle };
    userActionHeadersMock.mockResolvedValue({ 'X-User-Action': 'proof-of-user' });
    diagnosticsMock.mockResolvedValue(diagnosticsResponse() as never);
  });

  it('generates and saves the archive through the native diagnostics IPC', async () => {
    const onClose = vi.fn();
    saveDiagnosticsBundle.mockResolvedValue({
      canceled: false,
      filePath: '/Users/test/Downloads/diagnostics_20260716_27.zip',
    });

    render(<DiagnosticsModal isOpen onClose={onClose} sessionId="20260716_27" />);

    expect(screen.getByRole('dialog', { name: 'Report a Problem' })).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Generate diagnostics' }));

    await waitFor(() => {
      expect(saveDiagnosticsBundle).toHaveBeenCalledWith('20260716_27', expect.any(ArrayBuffer));
    });
    expect(diagnosticsMock).toHaveBeenCalledWith({
      headers: { 'X-User-Action': 'proof-of-user' },
      path: { session_id: '20260716_27' },
      throwOnError: true,
    });
    expect(toastSuccessMock).toHaveBeenCalledWith({
      title: 'Diagnostics saved',
      msg: 'The diagnostics bundle was saved to /Users/test/Downloads/diagnostics_20260716_27.zip.',
    });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('keeps the dialog open and recovers its controls when saving is canceled', async () => {
    const onClose = vi.fn();
    let finishSave: ((value: { canceled: true }) => void) | undefined;
    saveDiagnosticsBundle.mockImplementation(
      () =>
        new Promise((resolve) => {
          finishSave = resolve;
        })
    );

    render(<DiagnosticsModal isOpen onClose={onClose} sessionId="20260716_27" />);
    fireEvent.click(screen.getByRole('button', { name: 'Generate diagnostics' }));

    expect(await screen.findByRole('status')).toHaveTextContent('Preparing diagnostics bundle');
    expect(screen.getByRole('button', { name: 'Generating...' })).toBeDisabled();

    finishSave?.({ canceled: true });

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Generate diagnostics' })).toBeEnabled();
    });
    expect(screen.getByRole('dialog', { name: 'Report a Problem' })).toBeVisible();
    expect(onClose).not.toHaveBeenCalled();
    expect(toastSuccessMock).not.toHaveBeenCalled();
  });

  it('shows the native save failure without dismissing the dialog', async () => {
    const onClose = vi.fn();
    saveDiagnosticsBundle.mockResolvedValue({
      canceled: false,
      error: 'The diagnostics response is not a valid ZIP archive.',
    });

    render(<DiagnosticsModal isOpen onClose={onClose} sessionId="20260716_27" />);
    fireEvent.click(screen.getByRole('button', { name: 'Generate diagnostics' }));

    await waitFor(() => {
      expect(toastErrorMock).toHaveBeenCalledWith({
        title: 'Diagnostics error',
        msg: 'The diagnostics response is not a valid ZIP archive.',
      });
    });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog', { name: 'Report a Problem' })).toBeVisible();
  });

  it('shows a server refusal instead of replacing it with a generic error', async () => {
    const onClose = vi.fn();
    diagnosticsMock.mockRejectedValue('The diagnostics request was refused.');

    render(<DiagnosticsModal isOpen onClose={onClose} sessionId="20260716_27" />);
    fireEvent.click(screen.getByRole('button', { name: 'Generate diagnostics' }));

    await waitFor(() => {
      expect(toastErrorMock).toHaveBeenCalledWith({
        title: 'Diagnostics error',
        msg: 'The diagnostics request was refused.',
      });
    });
    expect(saveDiagnosticsBundle).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });
});
