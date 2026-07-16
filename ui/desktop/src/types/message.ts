import { Message, MessageEvent, ActionRequired, ToolRequest, ToolResponse } from '../api';

export type ToolRequestMessageContent = ToolRequest & { type: 'toolRequest' };
export type ToolResponseMessageContent = ToolResponse & { type: 'toolResponse' };
export type NotificationEvent = Extract<MessageEvent, { type: 'Notification' }>;

// Compaction response message - must match backend constant
const COMPACTION_THINKING_TEXT = 'biorouter is compacting the conversation...';

export type UserAttachment = { path: string; kind: 'image' };

export async function createUserMessage(
  text: string,
  attachments: UserAttachment[] = []
): Promise<Message> {
  const imageBlocks = await Promise.all(
    attachments
      .filter((a) => a.kind === 'image')
      .map(async (a) => {
        const { data, mimeType } = await window.electron.readTempImageAsBase64(a.path);
        return { type: 'image' as const, data, mimeType };
      })
  );

  const content: Message['content'] = [];
  if (text.trim().length > 0) {
    content.push({ type: 'text', text });
  }
  for (const block of imageBlocks) {
    content.push(block);
  }

  return {
    id: generateMessageId(),
    role: 'user',
    created: Math.floor(Date.now() / 1000),
    content,
    metadata: { userVisible: true, agentVisible: true },
  };
}

export function createElicitationResponseMessage(
  elicitationId: string,
  userData: Record<string, unknown>
): Message {
  return {
    id: generateMessageId(),
    role: 'user',
    created: Math.floor(Date.now() / 1000),
    content: [
      {
        type: 'actionRequired',
        data: {
          actionType: 'elicitationResponse',
          id: elicitationId,
          user_data: userData,
        },
      },
    ],
    metadata: { userVisible: false, agentVisible: true },
  };
}

export function createArtifactRenderRepairMessage({
  artifactTitle,
  message,
  detail,
  href,
}: {
  artifactTitle: string;
  message: string;
  detail?: string;
  href?: string;
}): Message {
  const text = [
    '[Biorouter artifact render guardrail]',
    'A system-generated artifact failed to render in the Biorouter preview.',
    `Artifact: ${artifactTitle}`,
    `Runtime error: ${message}`,
    detail ? `Details:\n${detail}` : null,
    href ? `Artifact URL: ${href}` : null,
    'Repair policy: inspect the generated artifact code, fix the runtime error, and render a corrected artifact. Only stop if there is a specific reason the artifact cannot be repaired.',
  ]
    .filter(Boolean)
    .join('\n\n');

  return {
    id: generateMessageId(),
    role: 'user',
    created: Math.floor(Date.now() / 1000),
    content: [{ type: 'text', text }],
    metadata: { userVisible: false, agentVisible: true },
  };
}

export function generateMessageId(): string {
  return Math.random().toString(36).substring(2, 10);
}

export function getTextContent(message: Message): string {
  return message.content
    .map((content) => {
      if (content.type === 'text') return content.text;
      return '';
    })
    .join('');
}

export function getToolRequests(message: Message): (ToolRequest & { type: 'toolRequest' })[] {
  return message.content.filter(
    (content): content is ToolRequest & { type: 'toolRequest' } => content.type === 'toolRequest'
  );
}

export function getToolResponses(message: Message): (ToolResponse & { type: 'toolResponse' })[] {
  return message.content.filter(
    (content): content is ToolResponse & { type: 'toolResponse' } => content.type === 'toolResponse'
  );
}

export function getToolConfirmationContent(
  message: Message
): (ActionRequired & { type: 'actionRequired' }) | undefined {
  return message.content.find(
    (content): content is ActionRequired & { type: 'actionRequired' } =>
      content.type === 'actionRequired' && content.data.actionType === 'toolConfirmation'
  );
}

export function getElicitationContent(
  message: Message
): (ActionRequired & { type: 'actionRequired' }) | undefined {
  return message.content.find(
    (content): content is ActionRequired & { type: 'actionRequired' } =>
      content.type === 'actionRequired' && content.data.actionType === 'elicitation'
  );
}

export function hasCompletedToolCalls(message: Message): boolean {
  const toolRequests = getToolRequests(message);
  return toolRequests.length > 0;
}

export function getThinkingMessage(message: Message | undefined): string | undefined {
  if (!message || message.role !== 'assistant') {
    return undefined;
  }

  for (const content of message.content) {
    if (content.type === 'systemNotification' && content.notificationType === 'thinkingMessage') {
      return content.msg;
    }
  }

  return undefined;
}

export function getCompactingMessage(message: Message | undefined): string | undefined {
  if (!message || message.role !== 'assistant') {
    return undefined;
  }

  for (const content of message.content) {
    if (content.type === 'systemNotification' && content.notificationType === 'thinkingMessage') {
      if (content.msg === COMPACTION_THINKING_TEXT) {
        return content.msg;
      }
    }
  }

  return undefined;
}
