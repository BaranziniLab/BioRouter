import React, { useState, useEffect } from 'react';
import {
  Calendar,
  MessageSquareText,
  Folder,
  Share2,
  Sparkles,
  Copy,
  Check,
  Target,
  LoaderCircle,
  AlertCircle,
} from '../icons/app-icons';
import { resumeSession } from '../../sessions';
import { Button } from '../ui/button';
import { toastError } from '../../toasts';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { ScrollArea } from '../ui/scroll-area';
import { formatMessageTimestamp } from '../../utils/timeUtils';
import { createSharedSession } from '../../sharedSessions';
import { billedSessionTokenEstimate, formatBilledTokenEstimate } from '../../utils/billedTokens';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import ProgressiveMessageList from '../ProgressiveMessageList';
import { SearchView } from '../conversation/SearchView';
import BackButton from '../ui/BackButton';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import { Message, Session } from '../../api';
import { useNavigation } from '../../hooks/useNavigation';
import { ReadableContent } from '../Layout/ReadableContent';
import { EmptyState } from '../ui/empty-state';

const isUserMessage = (message: Message): boolean => {
  if (message.role === 'assistant') {
    return false;
  }
  return !message.content.every((c) => c.type === 'toolConfirmationRequest');
};

const filterMessagesForDisplay = (messages: Message[]): Message[] => {
  return messages;
};

interface SessionHistoryViewProps {
  session: Session;
  isLoading: boolean;
  error: string | null;
  onBack: () => void;
  onRetry: () => void;
  showActionButtons?: boolean;
}

// Custom SessionHeader component similar to SessionListView style
const SessionHeader: React.FC<{
  onBack: () => void;
  children: React.ReactNode;
  title: string;
  actionButtons?: React.ReactNode;
}> = ({ onBack, children, title, actionButtons }) => {
  return (
    <div className="biorouter-page-header -mx-8 flex flex-col px-8 pb-8">
      <div className="flex items-center pt-0 mb-1">
        <BackButton onClick={onBack} />
      </div>
      {/* §4.2 — `text-title` carries the 24/600/-0.01em the three utilities
          beside it were spelling out by hand. */}
      <h1 className="text-title mb-4 pt-6">{title}</h1>
      <div className="flex items-center">{children}</div>
      {actionButtons && <div className="flex items-center space-x-3 mt-4">{actionButtons}</div>}
    </div>
  );
};

const SessionMessages: React.FC<{
  messages: Message[];
  isLoading: boolean;
  error: string | null;
  onRetry: () => void;
}> = ({ messages, isLoading, error, onRetry }) => {
  const filteredMessages = filterMessagesForDisplay(messages);

  return (
    <ScrollArea className="h-full w-full">
      <div className="pb-24 pt-8">
        <div className="flex flex-col space-y-6">
          {isLoading ? (
            <div className="flex justify-center items-center py-12">
              <LoaderCircle className="animate-spin h-8 w-8 text-text-default" />
            </div>
          ) : error ? (
            // §4.5 — the same shared surface the list view's error uses, so the
            // two halves of one feature stop speaking different error dialects.
            <EmptyState
              icon={AlertCircle}
              title="Couldn't load this session"
              description={error}
              actions={
                <Button onClick={onRetry} variant="outline">
                  Try again
                </Button>
              }
            />
          ) : filteredMessages?.length > 0 ? (
            <div className="max-w-4xl mx-auto w-full">
              <SearchView placeholder="Search history...">
                <ProgressiveMessageList
                  messages={filteredMessages}
                  chat={{
                    sessionId: 'session-preview',
                  }}
                  toolCallNotifications={new Map()}
                  append={() => {}} // Read-only for session history
                  isUserMessage={isUserMessage} // Use the same function as BaseChat
                  batchSize={15} // Same as BaseChat default
                  batchDelay={30} // Same as BaseChat default
                  showLoadingThreshold={30} // Same as BaseChat default
                />
              </SearchView>
            </div>
          ) : (
            <EmptyState
              icon={MessageSquareText}
              title="No messages in this session"
              description="This session was created but nothing was ever said in it."
              compact
            />
          )}
        </div>
      </div>
    </ScrollArea>
  );
};

const SessionHistoryView: React.FC<SessionHistoryViewProps> = ({
  session,
  isLoading,
  error,
  onBack,
  onRetry,
  showActionButtons = true,
}) => {
  const [isShareModalOpen, setIsShareModalOpen] = useState(false);
  const [shareLink, setShareLink] = useState<string>('');
  const [isSharing, setIsSharing] = useState(false);
  const [isCopied, setIsCopied] = useState(false);
  const [canShare, setCanShare] = useState(false);

  const messages = session.conversation || [];
  const billedTokenEstimate = billedSessionTokenEstimate(session);

  const setView = useNavigation();

  useEffect(() => {
    const savedSessionConfig = localStorage.getItem('session_sharing_config');
    if (savedSessionConfig) {
      try {
        const config = JSON.parse(savedSessionConfig);
        if (config.enabled && config.baseUrl) {
          setCanShare(true);
        }
      } catch (error) {
        console.error('Error parsing session sharing config:', error);
      }
    }
  }, []);

  const handleShare = async () => {
    setIsSharing(true);

    try {
      const savedSessionConfig = localStorage.getItem('session_sharing_config');
      if (!savedSessionConfig) {
        throw new Error('Session sharing is not configured. Please configure it in settings.');
      }

      const config = JSON.parse(savedSessionConfig);
      if (!config.enabled || !config.baseUrl) {
        throw new Error('Session sharing is not enabled or base URL is not configured.');
      }

      const shareToken = await createSharedSession(
        config.baseUrl,
        session.working_dir,
        messages,
        session.name || 'Shared Session',
        billedTokenEstimate?.lowerBound ? null : (billedTokenEstimate?.value ?? null)
      );

      const shareableLink = `biorouter://sessions/${shareToken}`;
      setShareLink(shareableLink);
      setIsShareModalOpen(true);
    } catch (error) {
      console.error('Error sharing session:', error);
      toastError({
        title: 'Failed to share session',
        msg: error instanceof Error ? error.message : 'Unknown error',
      });
    } finally {
      setIsSharing(false);
    }
  };

  const handleCopyLink = () => {
    navigator.clipboard
      .writeText(shareLink)
      .then(() => {
        setIsCopied(true);
        setTimeout(() => setIsCopied(false), 2000);
      })
      .catch((err) => {
        console.error('Failed to copy link:', err);
        toastError({
          title: 'Failed to copy link',
          msg: 'The session link could not be copied to the clipboard.',
        });
      });
  };

  const handleResumeSession = () => {
    try {
      resumeSession(session, setView);
    } catch (error) {
      toastError({
        title: 'Could not launch session',
        msg: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const actionButtons = showActionButtons ? (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            onClick={handleShare}
            disabled={!canShare || isSharing}
            size="sm"
            variant="outline"
            className={canShare ? '' : 'cursor-not-allowed opacity-50'}
          >
            {isSharing ? (
              <>
                <LoaderCircle className="w-4 h-4 mr-2 animate-spin" />
                Sharing...
              </>
            ) : (
              <>
                <Share2 className="w-4 h-4" />
                Share
              </>
            )}
          </Button>
        </TooltipTrigger>
        {!canShare ? (
          <TooltipContent>
            <p>
              To enable session sharing, go to <b>Settings</b> {'>'} <b>Session</b> {'>'}{' '}
              <b>Session Sharing</b>.
            </p>
          </TooltipContent>
        ) : null}
      </Tooltip>
      <Button onClick={handleResumeSession} size="sm" variant="outline">
        <Sparkles className="w-4 h-4" />
        Resume
      </Button>
    </>
  ) : null;

  return (
    <>
      <MainPanelLayout>
        <ReadableContent className="flex-1 flex flex-col min-h-0 px-8">
          <SessionHeader
            onBack={onBack}
            title={session.name}
            actionButtons={!isLoading ? actionButtons : null}
          >
            <div className="flex flex-col">
              {!isLoading ? (
                <>
                  <div className="flex items-center text-text-muted text-supporting gap-5 font-mono tabular-nums">
                    <span className="flex items-center">
                      <Calendar className="w-4 h-4 mr-1" />
                      {formatMessageTimestamp(messages[0]?.created)}
                    </span>
                    <span className="flex items-center">
                      <MessageSquareText className="w-4 h-4 mr-1" />
                      {session.message_count}
                    </span>
                    {billedTokenEstimate && (
                      <span
                        className="flex items-center"
                        title={
                          billedTokenEstimate.lowerBound
                            ? 'At least this many tokens; only last-turn usage is available for this legacy session'
                            : 'Billed tokens across every turn, including recorded cache usage'
                        }
                      >
                        <Target className="w-4 h-4 mr-1" />
                        <span className="sr-only">Billed tokens: </span>
                        {formatBilledTokenEstimate(billedTokenEstimate)}
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
                  <span>Loading session details...</span>
                </div>
              )}
            </div>
          </SessionHeader>

          <SessionMessages
            messages={messages}
            isLoading={isLoading}
            error={error}
            onRetry={onRetry}
          />
        </ReadableContent>
      </MainPanelLayout>

      <Dialog open={isShareModalOpen} onOpenChange={setIsShareModalOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="flex justify-center items-center gap-2">
              <Share2 className="w-6 h-6 text-text-default" />
              Share Session (beta)
            </DialogTitle>
            <DialogDescription>
              Share this session link to give others a read only view of your biorouter chat.
            </DialogDescription>
          </DialogHeader>

          <div className="py-4">
            <div className="relative rounded-container border border-border-subtle px-3 py-2 flex items-center bg-background-medium">
              <code className="text-code text-text-default overflow-x-hidden break-all pr-8 w-full">
                {shareLink}
              </code>
              <Button
                shape="pill"
                variant="ghost"
                className="absolute right-2 top-1/2 -translate-y-1/2"
                onClick={handleCopyLink}
                disabled={isCopied}
              >
                {isCopied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
                <span className="sr-only">Copy</span>
              </Button>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setIsShareModalOpen(false)}>
              Cancel
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
};

export default SessionHistoryView;
