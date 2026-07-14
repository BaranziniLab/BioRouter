import 'react-toastify/dist/ReactToastify.css';

import { ChatType } from '../types/chat';
import { UserAttachment } from '../types/message';
import BaseChat from './BaseChat';

export interface PairRouteState {
  resumeSessionId?: string;
  initialMessage?: string;
  initialAttachments?: UserAttachment[];
}

interface PairProps {
  setChat: (chat: ChatType) => void;
  sessionId: string;
  initialMessage?: string;
  initialAttachments?: UserAttachment[];
}

export default function Pair({
  setChat,
  sessionId,
  initialMessage,
  initialAttachments,
}: PairProps) {
  return (
    <BaseChat
      setChat={setChat}
      sessionId={sessionId}
      initialMessage={initialMessage}
      initialAttachments={initialAttachments}
      suppressEmptyState={false}
    />
  );
}
