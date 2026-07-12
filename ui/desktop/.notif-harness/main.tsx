import React, { useEffect } from 'react';
import { createRoot } from 'react-dom/client';
import { MemoryRouter } from 'react-router-dom';
import { ToastContainer } from 'react-toastify';
import './harness.css';
import 'react-toastify/dist/ReactToastify.css';

import { toastSuccess, toastError, toastLoading, toastService } from '../src/toasts';
import { NotificationSurface, NotificationContent } from '../src/components/alerts/NotificationSurface';
import { GroupedExtensionLoadingToast } from '../src/components/GroupedExtensionLoadingToast';
import { Button } from '../src/components/ui/button';

// A faithful copy of the real toast pill (App.tsx toastClassName + the
// .Toastify__toast-body gutter) so the inline column exercises the same layout
// without react-toastify's auto-close/animation.
function ToastFrame({ children }: { children: React.ReactNode }) {
  return (
    <div
      className="relative mb-3 flex items-start overflow-hidden rounded-xl border border-border-subtle bg-background-default p-3 text-text-default shadow-popover"
      style={{ width: 450 }}
    >
      <div style={{ display: 'block', flex: '1 1 auto', minWidth: 0, paddingInlineEnd: 28 }}>
        {children}
      </div>
      <button
        aria-label="close"
        className="absolute inline-flex items-center justify-center rounded-sm text-text-muted"
        style={{ right: 10, top: 12, width: 20, height: 20, opacity: 0.7 }}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>
  );
}

const FAILED_EXTS = [
  { name: 'developer', status: 'success' as const },
  {
    name: 'spoke-knowledge-graph',
    status: 'error' as const,
    error: 'Failed to add extension: SPOKEAGENT_PASSCODE not set',
    recoverHints: 'Set the SPOKE passcode',
  },
  {
    name: 'ucsf-omop-agent',
    status: 'error' as const,
    error: 'Failed to add extension: connection timed out after 30s',
  },
];

function Column({ theme }: { theme: 'light' | 'dark' }) {
  return (
    <div
      className={theme === 'dark' ? 'dark' : undefined}
      style={{ background: 'var(--background-muted)', padding: 20, borderRadius: 16, width: 500 }}
    >
      <div
        className="text-text-subtle"
        style={{ fontSize: 11, letterSpacing: '.08em', textTransform: 'uppercase', marginBottom: 14 }}
      >
        {theme} theme
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <ToastFrame>
          <NotificationContent status="loading" title="Loading extensions…" message="Starting Developer, Memory, SPOKE…" />
        </ToastFrame>
        <ToastFrame>
          <NotificationContent status="success" message="“Diabetes cohort dashboard” has been saved and is ready to use." />
        </ToastFrame>
        <ToastFrame>
          <NotificationContent status="success" title="Model changed" message="Now using Claude Opus 4.8 (Anthropic)" />
        </ToastFrame>
        <ToastFrame>
          <NotificationContent
            status="error"
            title="2 extensions failed to load"
            message="Failed: SPOKE Knowledge Graph, UCSF OMOP Agent"
            actions={
              <>
                <Button size="sm" variant="secondary">Ask biorouter</Button>
                <Button size="sm" variant="secondary">Copy error</Button>
              </>
            }
          />
        </ToastFrame>
        <ToastFrame>
          <GroupedExtensionLoadingToast extensions={FAILED_EXTS} totalCount={3} isComplete={true} />
        </ToastFrame>
        <NotificationSurface
          status="warning"
          message="This provider needs an API key before it can be used in a new chat."
        />
        <NotificationSurface
          status="info"
          title="Heads up"
          message="A newer CLI is available."
          onClose={() => {}}
        />
      </div>
    </div>
  );
}

function App() {
  useEffect(() => {
    // Fire real toasts through the real react-toastify path (exercises the
    // actual .Toastify__* CSS: close-button position + the single gutter).
    toastSuccess({ title: 'Model changed', msg: 'Now using Claude Opus 4.8 (Anthropic)', toastOptions: { autoClose: false } });
    toastLoading({ title: 'Loading extensions…', msg: 'Starting Developer, Memory, SPOKE…' });
    toastError({
      title: '2 extensions failed to load',
      msg: 'Failed: SPOKE Knowledge Graph, UCSF OMOP Agent',
      traceback: 'Error: SPOKEAGENT_PASSCODE not set',
      recoverHints: 'Set the SPOKE passcode',
    });
    toastService.extensionLoading(FAILED_EXTS, 3, true);
  }, []);

  return (
    <div style={{ background: '#efe9dd', minHeight: '100vh', padding: 28 }}>
      <h1 style={{ font: '600 20px/26px ui-sans-serif, system-ui', margin: '0 0 4px' }}>
        Notification harness — real components, real CSS
      </h1>
      <p style={{ font: '400 13px/19px ui-sans-serif, system-ui', color: '#635c54', margin: '0 0 20px' }}>
        Live toasts (top-right) exercise the real react-toastify close-button CSS. Columns below render the
        same components inline in both themes.
      </p>
      <div style={{ display: 'flex', gap: 22, flexWrap: 'wrap' }}>
        <Column theme="light" />
        <Column theme="dark" />
      </div>
      <ToastContainer
        aria-label="Toast notifications"
        toastClassName={() =>
          `relative mb-3 p-3 rounded-xl
               flex items-start overflow-hidden cursor-pointer
               text-text-default bg-background-default
               border border-border-subtle shadow-popover
              `
        }
        style={{ width: '450px' }}
        className="mt-6"
        position="top-right"
        autoClose={3000}
        closeOnClick
        pauseOnHover
      />
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <MemoryRouter>
    <App />
  </MemoryRouter>
);
