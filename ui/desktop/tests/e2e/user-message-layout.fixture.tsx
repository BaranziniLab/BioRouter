import React from 'react';
import { createRoot } from 'react-dom/client';
import type { Message } from '../../src/api';
import UserMessage from '../../src/components/UserMessage';
import { refTag } from '../../src/utils/resourceRefs';

Object.assign(window, {
  electron: {
    logInfo: () => undefined,
  },
});

const message = (id: string, text: string, extraContent: Message['content'] = []): Message => ({
  id,
  role: 'user',
  created: Math.floor(Date.now() / 1000),
  content: [{ type: 'text', text }, ...extraContent],
  metadata: { userVisible: true, agentVisible: true },
});

const cases: Array<{ id: string; label: string; value: Message }> = [
  { id: 'short', label: 'Short', value: message('short', 'hi?') },
  {
    id: 'medium',
    label: 'Medium',
    value: message('medium', 'Please compare these two result sets before we continue.'),
  },
  { id: 'token', label: 'Long token', value: message('token', '0123456789'.repeat(80)) },
  {
    id: 'clamped',
    label: 'Long and clamped',
    value: message(
      'clamped',
      Array.from({ length: 220 }, (_, index) => `trace line ${index}: run the next stage`).join(
        '\n'
      )
    ),
  },
  {
    id: 'resource',
    label: 'Resource chip',
    value: message('resource', `Use ${refTag('skill', 'literature review')}`),
  },
  {
    id: 'attachment',
    label: 'Attachment',
    value: message('attachment', 'See attachment', [
      {
        type: 'image',
        data: 'PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSI0MCIgaGVpZ2h0PSI0MCI+PHJlY3Qgd2lkdGg9IjQwIiBoZWlnaHQ9IjQwIiByeD0iOCIgZmlsbD0iI2Q5Nzc1NyIvPjwvc3ZnPg==',
        mimeType: 'image/svg+xml',
      },
    ]),
  },
  { id: 'edit', label: 'Edit state', value: message('edit', 'Edit me') },
];

function Fixture() {
  return (
    <main data-testid="transcript" className="mx-auto w-[900px] max-w-full px-6 py-8">
      {cases.map(({ id, label, value }) => (
        <section key={id} data-case={id} aria-label={label} className="mb-8 w-full">
          <UserMessage message={value} onMessageUpdate={() => undefined} />
        </section>
      ))}
    </main>
  );
}

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Fixture />
  </React.StrictMode>
);
