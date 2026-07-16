import { NotificationSurface } from '../alerts/NotificationSurface';
import type { ChatTurnErrorData } from '../../types/turnError';
import { isConnectionError } from '../../utils/conversionUtils';

interface ChatTurnErrorPresentation {
  title: string;
  message: string;
  details?: string;
}

const PROVIDER_TITLES: Record<string, string> = {
  auth: 'Model authentication failed',
  context_length: 'Model context limit exceeded',
  invalid_request: 'Model rejected the request',
  model_unavailable: 'Model unavailable',
  policy: 'Request blocked by provider policy',
  quota: 'Model quota exceeded',
  rate_limit: 'Model rate limit reached',
  server: 'Model provider unavailable',
};

function decodeProviderMessage(value: string): string {
  try {
    return JSON.parse(`"${value}"`) as string;
  } catch {
    return value.replace(/\\"/g, '"').replace(/\\n/g, '\n');
  }
}

function providerMessage(value: string): string | undefined {
  const encoded = value.match(/"message":\s*String\("((?:\\.|[^"\\])*)"\)/)?.[1];
  return encoded ? decodeProviderMessage(encoded) : undefined;
}

function userFacingMessage(error: ChatTurnErrorData): string {
  const decoded = providerMessage(error.message) ?? providerMessage(error.technicalDetails ?? '');
  if (decoded) return decoded;

  if (error.scope === 'transport' || isConnectionError(error.message)) {
    return 'Biorouter could not reach the model provider. Check your connection and provider settings, then try again.';
  }

  return (
    error.message
      .replace(/^(?:Stream|Submit) error:\s*/i, '')
      .replace(/^provider_failure:\s*/i, '')
      .trim() || 'The model request failed before it produced a response.'
  );
}

export function presentChatTurnError(error: ChatTurnErrorData): ChatTurnErrorPresentation {
  let title = 'Model request failed';
  if (error.providerKind && PROVIDER_TITLES[error.providerKind]) {
    title = PROVIDER_TITLES[error.providerKind];
  } else if (error.message.includes('insufficient_quota')) {
    title = 'Model quota exceeded';
  } else if (error.scope === 'transport' || isConnectionError(error.message)) {
    title = 'Model connection failed';
  } else if (error.scope === 'session') {
    title = 'Model could not be prepared';
  } else if (error.scope === 'internal') {
    title = 'Model turn ended unexpectedly';
  }

  const message = userFacingMessage(error);
  const details = error.technicalDetails?.trim();
  return {
    title,
    message,
    details: details && details !== message ? details : undefined,
  };
}

export function ChatTurnError({ error }: { error: ChatTurnErrorData }) {
  const presentation = presentChatTurnError(error);

  return (
    <div data-testid="chat-turn-error" className="mt-4">
      <NotificationSurface
        status="error"
        role="alert"
        title={presentation.title}
        message={presentation.message}
      >
        {presentation.details && (
          <details className="mt-2.5 text-xs text-text-muted">
            <summary className="w-fit cursor-pointer select-none font-medium text-text-default">
              Technical details
            </summary>
            <div className="mt-2 whitespace-pre-wrap break-words font-mono leading-relaxed [overflow-wrap:anywhere]">
              {presentation.details}
            </div>
          </details>
        )}
      </NotificationSurface>
    </div>
  );
}
