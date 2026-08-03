import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import SettingsView from './SettingsView';

// Every section but Privacy is stubbed: this file asserts the TAB STRIP and that
// the Privacy panel is actually mounted behind its trigger — which is exactly
// the failure `components/settings/security/SecurityToggle.tsx` is in today
// (declared, plausible, zero consumers repo-wide).
vi.mock('./models/ModelsSection', () => ({ default: () => <div /> }));
vi.mock('./chat/ChatSettingsSection', () => ({ default: () => <div /> }));
vi.mock('./app/AppSettingsSection', () => ({ default: () => <div /> }));
vi.mock('./app/WorkspaceSettingsSection', () => ({ WorkspaceSettingsSection: () => <div /> }));
vi.mock('./config/ConfigSettings', () => ({ default: () => <div /> }));

vi.mock('../ConfigContext', () => ({
  useConfig: () => ({
    read: vi.fn(async () => undefined),
    upsert: vi.fn(async () => undefined),
  }),
}));

describe('SettingsView', () => {
  it('the Privacy tab exists and its toggle is mounted', async () => {
    const user = userEvent.setup();
    render(<SettingsView onClose={() => {}} setView={() => {}} viewOptions={{}} />);

    await user.click(screen.getByTestId('settings-privacy-tab'));
    expect(await screen.findByRole('switch', { name: /Privacy tiers/ })).toBeInTheDocument();
  });
});
