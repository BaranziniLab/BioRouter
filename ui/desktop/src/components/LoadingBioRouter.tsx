import { ChatState } from '../types/chatState';

interface LoadingBioRouterProps {
  message?: string;
  chatState?: ChatState;
}

const STATE_MESSAGES: Record<ChatState, string> = {
  [ChatState.LoadingConversation]: 'Loading conversation…',
  [ChatState.Thinking]: 'Biorouter is thinking…',
  [ChatState.Streaming]: 'Biorouter is working on it…',
  [ChatState.WaitingForUserInput]: 'Biorouter is waiting…',
  [ChatState.Compacting]: 'Biorouter is compacting the conversation…',
  [ChatState.Idle]: 'Biorouter is working on it…',
  [ChatState.RestartingAgent]: 'Restarting session…',
};

const LoadingBioRouter = ({ message, chatState = ChatState.Idle }: LoadingBioRouterProps) => {
  const displayMessage = message || STATE_MESSAGES[chatState];

  return (
    <div className="w-full animate-fade-slide-up">
      <div
        data-testid="loading-biorouter"
        aria-live="polite"
        className="inline-flex items-center gap-2 rounded-full px-1 py-1 text-xs text-text-default/80"
      >
        <span
          aria-hidden="true"
          className="relative flex h-4 w-4 flex-shrink-0 items-center justify-center text-text-default/80"
        >
          <span className="absolute h-4 w-4 rounded-full border border-current animate-[biorouter-working-ring_1.8s_ease-out_infinite]" />
          <span className="absolute h-2.5 w-2.5 rounded-full bg-current opacity-20 animate-[biorouter-working-glow_1.8s_ease-in-out_infinite]" />
          <span className="h-1.5 w-1.5 rounded-full bg-current opacity-70" />
        </span>
        <span>{displayMessage}</span>
      </div>
    </div>
  );
};

export default LoadingBioRouter;
