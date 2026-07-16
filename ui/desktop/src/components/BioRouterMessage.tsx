import { useMemo, useRef } from 'react';
import ImagePreview from './ImagePreview';
import { InlineImage } from './InlineImage';
import { extractImagePaths, removeImagePathsFromText } from '../utils/imageUtils';
import { formatMessageTimestamp } from '../utils/timeUtils';
import MarkdownContent from './MarkdownContent';
import ToolCallWithResponse from './ToolCallWithResponse';
import {
  getTextContent,
  getToolRequests,
  getToolResponses,
  getToolConfirmationContent,
  getElicitationContent,
  NotificationEvent,
} from '../types/message';
import { Message } from '../api';
import ToolCallConfirmation from './ToolCallConfirmation';
import ElicitationRequest from './ElicitationRequest';
import MessageCopyLink from './MessageCopyLink';
import MessageDivergeLink from './MessageDivergeLink';
import { cn } from '../utils';
import { identifyConsecutiveToolCalls, shouldHideTimestamp } from '../utils/toolCallChaining';
import type { ArtifactSource } from './artifacts/artifactTypes';

interface BioRouterMessageProps {
  sessionId: string;
  message: Message;
  messages: Message[];
  metadata?: string[];
  toolCallNotifications: Map<string, NotificationEvent[]>;
  append: (value: string) => void;
  isStreaming?: boolean; // Whether this message is currently being streamed
  // Precomputed by the parent list once per render and passed down to avoid each
  // message recomputing O(n) scans over the whole conversation (which made the
  // list O(n²) per streaming frame). Both fall back to local computation when
  // omitted, so the component still works standalone.
  messageIndex?: number;
  toolCallChains?: ReturnType<typeof identifyConsecutiveToolCalls>;
  submitElicitationResponse?: (
    elicitationId: string,
    userData: Record<string, unknown>
  ) => Promise<void>;
  onOpenArtifact?: (artifact: ArtifactSource) => void;
  workingDir?: string;
}

export default function BioRouterMessage({
  sessionId,
  message,
  messages,
  toolCallNotifications,
  append,
  isStreaming = false,
  messageIndex: messageIndexProp,
  toolCallChains: toolCallChainsProp,
  submitElicitationResponse,
  onOpenArtifact,
  workingDir,
}: BioRouterMessageProps) {
  const contentRef = useRef<HTMLDivElement | null>(null);

  let textContent = getTextContent(message);

  // Strip injected context blocks that are meant for the model, not the user.
  // The trailing (\s*\\?") also consumes any closing \" the model places after </info-msg>
  // to wrap a quoted echo — which Markdown would otherwise render as a stray \.
  textContent = textContent.replace(/<info-msg>[\s\S]*?<\/info-msg>(\s*\\?")?/gi, '').trim();

  const splitChainOfThought = (text: string): { visibleText: string; cotText: string | null } => {
    const regex = /<think>([\s\S]*?)<\/think>/i;
    const match = text.match(regex);
    if (!match) {
      return { visibleText: text, cotText: null };
    }

    const cotRaw = match[1].trim();
    const visibleText = text.replace(regex, '').trim();

    return {
      visibleText,
      cotText: cotRaw || null,
    };
  };

  const { visibleText, cotText } = splitChainOfThought(textContent);
  const imagePaths = extractImagePaths(visibleText);
  const displayText =
    imagePaths.length > 0 ? removeImagePathsFromText(visibleText, imagePaths) : visibleText;

  // Extract structured ImageContent blocks (new sessions only; pre-v-next sessions have no image blocks)
  const imageContentBlocks = useMemo(
    () => message.content.filter((c) => c.type === 'image'),
    [message.content]
  );

  const timestamp = useMemo(() => formatMessageTimestamp(message.created), [message.created]);
  const toolRequests = getToolRequests(message);
  // Use the index passed by the parent list when available (O(1)); only fall
  // back to the O(n) scan when rendered standalone.
  const messageIndex = messageIndexProp ?? messages.findIndex((msg) => msg.id === message.id);
  const toolConfirmationContent = getToolConfirmationContent(message);
  const elicitationContent = getElicitationContent(message);
  // Prefer the parent's single precomputed chain map; only recompute locally
  // (the old O(n)-per-message cost) when not provided.
  const toolCallChains = useMemo(
    () => toolCallChainsProp ?? identifyConsecutiveToolCalls(messages),
    [toolCallChainsProp, messages]
  );
  const hideTimestamp = useMemo(
    () => shouldHideTimestamp(messageIndex, toolCallChains),
    [messageIndex, toolCallChains]
  );
  const hasToolConfirmation = toolConfirmationContent !== undefined;
  const hasElicitation = elicitationContent !== undefined;

  const toolResponsesMap = useMemo(() => {
    const responseMap = new Map();

    // BR-53: a message with no tool requests can never match a later tool
    // response, so skip the whole-conversation scan entirely. This memo ran
    // once per text-only message on every streamed token — O(n²) per frame —
    // purely to build an empty map. Guard on `toolRequests` first so the common
    // text-only message pays nothing.
    if (toolRequests.length > 0 && messageIndex !== undefined && messageIndex >= 0) {
      const requestIds = new Set(toolRequests.map((req) => req.id));
      for (let i = messageIndex + 1; i < messages.length; i++) {
        for (const response of getToolResponses(messages[i])) {
          if (requestIds.has(response.id)) {
            responseMap.set(response.id, response);
          }
        }
      }
    }

    return responseMap;
  }, [messages, messageIndex, toolRequests]);

  return (
    <div className="biorouter-message flex w-full justify-start min-w-0">
      <div className="flex flex-col w-full min-w-0">
        {cotText && (
          <details className="bg-background-medium border border-border-subtle rounded p-2 mb-2">
            <summary className="cursor-pointer text-sm text-text-muted select-none">
              Show thinking
            </summary>
            <div className="mt-2">
              <MarkdownContent content={cotText} />
            </div>
          </details>
        )}

        {displayText && (
          <div className="flex flex-col group">
            <div ref={contentRef} className="w-full">
              <MarkdownContent
                content={displayText}
                onOpenArtifact={onOpenArtifact}
                workingDir={workingDir}
              />
            </div>

            {imagePaths.length > 0 && (
              <div className="mt-4">
                {imagePaths.map((imagePath, index) => (
                  <ImagePreview key={index} src={imagePath} />
                ))}
              </div>
            )}

            {/* Render structured ImageContent blocks (new sessions) */}
            {imageContentBlocks.length > 0 && (
              <div className="flex flex-wrap gap-2 mt-4">
                {imageContentBlocks.map((block, idx) =>
                  block.type === 'image' ? (
                    <InlineImage
                      key={idx}
                      kind="data"
                      data={block.data}
                      mimeType={block.mimeType}
                    />
                  ) : null
                )}
              </div>
            )}

            {toolRequests.length === 0 && (
              <div className="relative flex justify-start">
                {!isStreaming && (
                  <div className="font-sans text-sm text-text-muted pt-1 transition-[transform,opacity] duration-150 group-hover:-translate-y-4 group-hover:opacity-0">
                    {timestamp}
                  </div>
                )}
                {message.content.every((content) => content.type === 'text') && !isStreaming && (
                  <div className="absolute left-0 pt-1 flex items-center gap-3">
                    <MessageCopyLink text={displayText} contentRef={contentRef} />
                    <MessageDivergeLink
                      sessionId={sessionId}
                      truncateAfterMs={message.created}
                      truncateAfterId={message.id ?? undefined}
                    />
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {toolRequests.length > 0 && (
          <div className={cn(displayText && 'mt-2')}>
            <div className="relative flex flex-col w-full">
              <div className="flex flex-col gap-3">
                {toolRequests.map((toolRequest) => (
                  <div className="biorouter-message-tool" key={toolRequest.id}>
                    <ToolCallWithResponse
                      sessionId={sessionId}
                      isCancelledMessage={false}
                      toolRequest={toolRequest}
                      toolResponse={toolResponsesMap.get(toolRequest.id)}
                      notifications={toolCallNotifications.get(toolRequest.id)}
                      isStreamingMessage={isStreaming}
                      append={append}
                      onOpenArtifact={onOpenArtifact}
                      workingDir={workingDir}
                    />
                  </div>
                ))}
              </div>
              <div className="font-sans text-sm text-text-muted transition-[transform,opacity] duration-150 group-hover:-translate-y-4 group-hover:opacity-0 pt-1">
                {!isStreaming && !hideTimestamp && timestamp}
              </div>
            </div>
          </div>
        )}

        {hasToolConfirmation && (
          <ToolCallConfirmation
            sessionId={sessionId}
            isCancelledMessage={false}
            isClicked={false}
            actionRequiredContent={toolConfirmationContent}
          />
        )}

        {hasElicitation && submitElicitationResponse && (
          <ElicitationRequest
            isCancelledMessage={false}
            isClicked={false}
            actionRequiredContent={elicitationContent}
            onSubmit={submitElicitationResponse}
          />
        )}
      </div>
    </div>
  );
}
