import React from 'react';
import { MessageSquare, AlertCircle, LoaderCircle } from '../icons/app-icons';
import { Card } from '../ui/card';
import { Button } from '../ui/button';
import { ScrollArea } from '../ui/scroll-area';
import MarkdownContent from '../MarkdownContent';
import { ResourceRefChip } from '../ResourceRefChip';
import { splitComposerText } from '../../utils/composerRefs';
import ToolCallWithResponse from '../ToolCallWithResponse';
import ImagePreview from '../ImagePreview';
import { ProvenanceChip } from '../ProvenanceChip';
import {
  getTextContent,
  ToolRequestMessageContent,
  ToolResponseMessageContent,
} from '../../types/message';
import { formatMessageTimestamp } from '../../utils/timeUtils';
import { extractImagePaths, removeImagePathsFromText } from '../../utils/imageUtils';
import { Message } from '../../api';
import { EmptyState } from '../ui/empty-state';

/**
 * Get tool responses map from messages
 */
export const getToolResponsesMap = (
  messages: Message[],
  messageIndex: number,
  toolRequests: ToolRequestMessageContent[]
) => {
  const responseMap = new Map();

  if (messageIndex >= 0) {
    for (let i = messageIndex + 1; i < messages.length; i++) {
      const responses = messages[i].content
        .filter((c) => c.type === 'toolResponse')
        .map((c) => c as ToolResponseMessageContent);

      for (const response of responses) {
        const matchingRequest = toolRequests.find((req) => req.id === response.id);
        if (matchingRequest) {
          responseMap.set(response.id, response);
        }
      }
    }
  }

  return responseMap;
};

interface SessionMessagesProps {
  messages: Message[];
  isLoading: boolean;
  error: string | null;
  onRetry: () => void;
}

/**
 * Common component for displaying session messages
 */
export const SessionMessages: React.FC<SessionMessagesProps> = ({
  messages,
  isLoading,
  error,
  onRetry,
}) => {
  return (
    <ScrollArea className="h-full w-full">
      <div className="p-4">
        <div className="flex flex-col space-y-4">
          <div className="space-y-4 mb-6">
            {isLoading ? (
              <div className="flex justify-center items-center py-12">
                <LoaderCircle className="h-6 w-6 animate-spin text-text-muted" aria-hidden="true" />
              </div>
            ) : error ? (
              // §4.5 — the shared surface, not a fourth hand-rolled error column.
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
            ) : messages?.length > 0 ? (
              messages
                .map((message, index) => {
                  const textContent = getTextContent(message);
                  // Extract image paths from the message
                  const imagePaths = extractImagePaths(textContent);

                  // Remove image paths from text for display
                  const displayText =
                    imagePaths.length > 0
                      ? removeImagePathsFromText(textContent, imagePaths)
                      : textContent;

                  // Issue #65 — this is the one surface that renders a user
                  // message through `MarkdownContent`, and react-markdown runs
                  // here without `rehype-raw`, so it DROPS unknown HTML rather
                  // than showing it. A `<biorouter-ref …>` therefore vanished
                  // without a trace: worse than raw markup, because a reader
                  // reviewing the session could not tell a skill was attached.
                  // The prose keeps its markdown; the references are drawn as
                  // read-only chips beside it.
                  const { body: proseText, refs: messageRefs } = splitComposerText(displayText);

                  // Get tool requests from the message
                  const toolRequests = message.content
                    .filter((c) => c.type === 'toolRequest')
                    .map((c) => c as ToolRequestMessageContent);

                  // Get tool responses map using the helper function
                  const toolResponsesMap = getToolResponsesMap(messages, index, toolRequests);

                  // Skip pure tool response messages for cleaner display
                  const isOnlyToolResponse =
                    message.content.length > 0 &&
                    message.content.every((c) => c.type === 'toolResponse');

                  if (message.role === 'user' && isOnlyToolResponse) {
                    return null;
                  }

                  return (
                    <Card
                      key={index}
                      className={`p-4 ${
                        message.role === 'user'
                          ? 'bg-bgSecondary border border-border-subtle'
                          : 'bg-background-medium'
                      }`}
                    >
                      <div className="flex justify-between items-center mb-2 gap-2">
                        {/* BR-71 §5: provenance is structural, so it has to
                            travel with the transcript into this view too — a
                            shared session is the one that leaves the machine,
                            and "You" on a message another agent injected is a
                            misattribution to the human. */}
                        <div className="flex items-center gap-2 min-w-0">
                          <span className="text-label text-text-default">
                            {message.role === 'user' ? 'You' : 'Biorouter'}
                          </span>
                          <ProvenanceChip provenance={message.metadata?.provenance ?? undefined} />
                        </div>
                        <span className="text-supporting text-text-muted shrink-0">
                          {formatMessageTimestamp(message.created)}
                        </span>
                      </div>

                      <div className="flex flex-col w-full">
                        {/* Text content */}
                        {proseText && (
                          <div
                            className={`${toolRequests.length > 0 || imagePaths.length > 0 || messageRefs.length > 0 ? 'mb-4' : ''}`}
                          >
                            <MarkdownContent content={proseText} />
                          </div>
                        )}

                        {messageRefs.length > 0 && (
                          <div className="mb-2 flex flex-wrap items-center gap-1.5">
                            {messageRefs.map((ref) => (
                              <ResourceRefChip key={`${ref.kind}:${ref.value}`} refSpan={ref} />
                            ))}
                          </div>
                        )}

                        {/* Render images if any */}
                        {imagePaths.length > 0 && (
                          <div className="flex flex-wrap gap-2 mt-2 mb-2">
                            {imagePaths.map((imagePath, imageIndex) => (
                              <ImagePreview
                                key={imageIndex}
                                src={imagePath}
                                alt={`Image ${imageIndex + 1}`}
                              />
                            ))}
                          </div>
                        )}

                        {/* Tool requests and responses */}
                        {toolRequests.length > 0 && (
                          <div className="biorouter-message-tool bg-background-default border border-border-subtle rounded-b-2xl px-4 pt-4 pb-2 mt-1">
                            {toolRequests.map((toolRequest) => (
                              <ToolCallWithResponse
                                // In the session history page, if no tool response found for given request, it means the tool call
                                // is broken or cancelled.
                                isCancelledMessage={
                                  toolResponsesMap.get(toolRequest.id) == undefined
                                }
                                key={toolRequest.id}
                                toolRequest={toolRequest}
                                toolResponse={toolResponsesMap.get(toolRequest.id)}
                              />
                            ))}
                          </div>
                        )}
                      </div>
                    </Card>
                  );
                })
                .filter(Boolean) // Filter out null entries
            ) : (
              <EmptyState
                icon={MessageSquare}
                title="No messages in this session"
                description="This session was created but nothing was ever said in it."
                compact
              />
            )}
          </div>
        </div>
      </div>
    </ScrollArea>
  );
};
