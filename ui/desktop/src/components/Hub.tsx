/**
 * Hub Component
 *
 * The Hub is the main landing page and entry point for the BioRouter Desktop application.
 * It serves as the welcome screen where users can start new conversations.
 *
 * Key Responsibilities:
 * - Displays SessionInsights to show session statistics and recent chats
 * - Provides a ChatInput for users to start new conversations
 * - Creates a new session and navigates to Pair with the session ID
 * - Shows loading state while session is being created
 *
 * Navigation Flow:
 * Hub (input submission) → Create Session → Pair (with session ID and initial message)
 */

import { useLayoutEffect, useRef, useState } from 'react';
import { SessionInsights } from './sessions/SessionsInsights';
import ChatInput from './ChatInput';
import { ChatState } from '../types/chatState';
import 'react-toastify/dist/ReactToastify.css';
import { View, ViewOptions } from '../utils/navigationUtils';
import { useConfig } from './ConfigContext';
import {
  getExtensionConfigsWithOverrides,
  clearExtensionOverrides,
} from '../store/extensionOverrides';
import { getInitialWorkingDir } from '../utils/workingDir';
import { createSession } from '../sessions';
import LoadingBioRouter from './LoadingBioRouter';
import type { UserAttachment } from '../types/message';

export default function Hub({
  setView,
}: {
  setView: (view: View, viewOptions?: ViewOptions) => void;
}) {
  const { extensionsList } = useConfig();
  const [workingDir, setWorkingDir] = useState(getInitialWorkingDir());
  const [isCreatingSession, setIsCreatingSession] = useState(false);

  // The composer floats over the bottom of the scrollable content (absolute,
  // so clicks in its gutter fall through). Its height is dynamic — the greeting
  // above it can wrap to two lines and it grows as the user types — so reserve
  // exactly that much space at the foot of the scroll area. Without this the
  // last recent-chat row slides under the composer and is clipped.
  const composerRef = useRef<HTMLDivElement>(null);
  const [composerInset, setComposerInset] = useState(200);
  useLayoutEffect(() => {
    const el = composerRef.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const measure = () => setComposerInset(el.offsetHeight + 48); // + bottom-6 gap + buffer
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [isCreatingSession]);

  const handleSubmit = async (e: React.FormEvent) => {
    const customEvent = e as unknown as CustomEvent;
    const combinedTextFromInput = customEvent.detail?.value || '';
    const attachments = (customEvent.detail?.attachments ?? []) as UserAttachment[];
    const hasAttachments = attachments.length > 0;

    if ((combinedTextFromInput.trim() || hasAttachments) && !isCreatingSession) {
      const extensionConfigs = getExtensionConfigsWithOverrides(extensionsList);
      clearExtensionOverrides();
      setIsCreatingSession(true);

      try {
        const session = await createSession(workingDir, {
          extensionConfigs,
          allExtensions: extensionConfigs.length > 0 ? undefined : extensionsList,
        });

        setView('pair', {
          resumeSessionId: session.id,
          initialMessage: combinedTextFromInput,
          initialAttachments: attachments,
        });
      } catch (error) {
        console.error('Failed to create session:', error);
        setIsCreatingSession(false);
      }

      e.preventDefault();
    }
  };

  return (
    <div className="relative flex flex-col h-full bg-background-muted">
      <div
        className="flex-1 min-h-0 overflow-y-auto"
        style={{ paddingBottom: `${composerInset}px` }}
      >
        <SessionInsights />
      </div>

      <div
        ref={composerRef}
        className="absolute inset-x-4 sm:inset-x-6 bottom-6 z-10 pointer-events-none"
      >
        <div className="biorouter-composer-view-transition w-full max-w-[760px] mx-auto pointer-events-auto">
          {isCreatingSession && (
            <div className="mb-2.5 pl-2 pointer-events-none">
              <LoadingBioRouter chatState={ChatState.LoadingConversation} />
            </div>
          )}
          <ChatInput
            sessionId={null}
            handleSubmit={handleSubmit}
            chatState={isCreatingSession ? ChatState.LoadingConversation : ChatState.Idle}
            onStop={() => {}}
            initialValue=""
            setView={setView}
            totalTokens={0}
            accumulatedInputTokens={0}
            accumulatedOutputTokens={0}
            droppedFiles={[]}
            onFilesProcessed={() => {}}
            messages={[]}
            disableAnimation={false}
            sessionCosts={undefined}
            toolCount={0}
            onWorkingDirChange={setWorkingDir}
          />
        </div>
      </div>
    </div>
  );
}
