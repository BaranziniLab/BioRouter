import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  success: vi.fn(),
}));

// react-toastify is the only thing `toastService.success` actually reaches; stub
// the whole module so the assertions are about the options we hand it.
vi.mock('react-toastify', () => ({
  toast: Object.assign(vi.fn(), {
    success: mocks.success,
    error: vi.fn(),
    info: vi.fn(),
    warning: vi.fn(),
    loading: vi.fn(),
    update: vi.fn(),
    dismiss: vi.fn(),
    isActive: vi.fn(() => false),
  }),
}));

import { toastService } from './toasts';

describe('toastService.success', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    toastService.configure({ silent: false });
  });

  it('keeps the shared 3s auto-close by default', () => {
    toastService.success({ title: 'Saved', msg: 'All good' });

    expect(mocks.success).toHaveBeenCalledTimes(1);
    expect(mocks.success.mock.calls[0][1]).toMatchObject({ autoClose: 3000 });
  });

  // BR-71 §3.2 (decision 14): the chatrecall suggestion is shown exactly once in
  // the lifetime of an install, so it must not be able to expire unread. That is
  // only possible if a caller can override the 3s default per toast.
  it('forwards per-toast options so a caller can opt out of auto-close', () => {
    toastService.success(
      { title: 'Workspace Control enabled', msg: 'long copy' },
      {
        autoClose: false,
      }
    );

    expect(mocks.success).toHaveBeenCalledTimes(1);
    expect(mocks.success.mock.calls[0][1]).toMatchObject({ autoClose: false });
  });
});
