import React from 'react';
import {
  Calendar,
  MessageSquareText,
  Folder,
  Target,
  LoaderCircle,
  Share2,
} from '../icons/app-icons';
import { type SharedSessionDetails } from '../../sharedSessions';
import { SessionMessages } from './SessionViewComponents';
import { formatMessageTimestamp } from '../../utils/timeUtils';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import ArtifactViewer from '../artifacts/ArtifactViewer';
import { useArtifactPanel } from '../artifacts/useArtifactPanel';
import { useIsMobile } from '../../hooks/use-mobile';

interface SharedSessionViewProps {
  session: SharedSessionDetails | null;
  isLoading: boolean;
  error: string | null;
  onRetry: () => void;
}

// Custom SessionHeader component matching SessionHistoryView style
const SessionHeader: React.FC<{
  children: React.ReactNode;
  title: string;
}> = ({ children, title }) => {
  return (
    <div className="biorouter-page-header -mx-8 flex flex-col px-8 pb-8">
      <h1 className="text-title mb-4 pt-6">{title}</h1>
      <div className="flex items-center">{children}</div>
    </div>
  );
};

const SharedSessionView: React.FC<SharedSessionViewProps> = ({
  session,
  isLoading,
  error,
  onRetry,
}) => {
  // The same panel the live chat and the saved-session page mount. A shared
  // transcript is the one that leaves the machine, and it gets the identical
  // figure surface — the alternative was an inline iframe here and a panel
  // everywhere else, which is how the same figure came to look like two
  // different things depending on where you opened it.
  const artifactPanel = useArtifactPanel({ isMobile: useIsMobile(), allowWindowResize: false });
  const { splitPaneRef, artifact: presentedArtifact, openArtifact } = artifactPanel;

  return (
    <MainPanelLayout>
      <div ref={splitPaneRef} className="relative flex flex-1 min-h-0 min-w-0">
        <div className="flex-1 flex flex-col min-h-0 px-8">
          <div className="biorouter-page-header -mx-8 mb-6 flex items-center px-8 py-4">
            <div className="flex items-center text-text-muted">
              <Share2 className="w-5 h-5 mr-2" />
              <span className="text-label">Shared chat</span>
            </div>
          </div>

          <SessionHeader title={session ? session.description : 'Shared chat'}>
            <div className="flex flex-col">
              {!isLoading && session && session.messages.length > 0 ? (
                <>
                  <div className="flex items-center text-text-muted text-supporting gap-5 font-mono tabular-nums">
                    <span className="flex items-center">
                      <Calendar className="w-4 h-4 mr-1" />
                      {formatMessageTimestamp(session.messages[0]?.created)}
                    </span>
                    <span className="flex items-center">
                      <MessageSquareText className="w-4 h-4 mr-1" />
                      {session.message_count}
                    </span>
                    {session.total_tokens !== null && (
                      <span
                        className="flex items-center"
                        title="Billed tokens across every turn, not only the last message"
                      >
                        <Target className="w-4 h-4 mr-1" />
                        Billed tokens: {session.total_tokens.toLocaleString()}
                      </span>
                    )}
                  </div>
                  <div className="flex items-center text-text-muted text-supporting mt-1 font-mono">
                    <span className="flex items-center">
                      <Folder className="w-4 h-4 mr-1" />
                      {session.working_dir}
                    </span>
                  </div>
                </>
              ) : (
                <div className="flex items-center text-secondary text-text-muted">
                  <LoaderCircle className="w-4 h-4 mr-2 animate-spin" />
                  <span>Loading chat details...</span>
                </div>
              )}
            </div>
          </SessionHeader>

          <SessionMessages
            messages={session?.messages || []}
            sessionId={session ? `shared:${session.share_token}` : 'shared:loading'}
            isLoading={isLoading}
            error={error}
            onRetry={onRetry}
            onOpenArtifact={openArtifact}
            workingDir={session?.working_dir}
          />
        </div>

        {/* Read-only: no `onRenderError`, so no repair listener, and nothing
            auto-opens. */}
        {presentedArtifact && <ArtifactViewer {...artifactPanel.viewerProps} />}
      </div>
    </MainPanelLayout>
  );
};

export default SharedSessionView;
