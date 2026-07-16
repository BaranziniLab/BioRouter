import { NotificationSurface } from '../alerts/NotificationSurface';
import { isConnectionError } from '../../utils/conversionUtils';

interface ChatTurnErrorPresentation {
  title: string;
  message: string;
  details?: string;
}

function decodeProviderMessage(value: string): string {
  try {
    return JSON.parse(`"${value}"`) as string;
  } catch {
    return value.replace(/\\"/g, '"').replace(/\\n/g, '\n');
  }
}

export function presentChatTurnError(error: string): ChatTurnErrorPresentation {
  const raw = error.trim();
  const isSubmitError = /^Submit error:/i.test(raw);
  const withoutTransportPrefix = raw.replace(/^(?:Stream|Submit) error:\s*/i, '');
  const providerMessage = raw.match(/"message":\s*String\("((?:\\.|[^"\\])*)"\)/)?.[1];

  if (raw.includes('insufficient_quota')) {
    return {
      title: 'Model quota exceeded',
      message: providerMessage
        ? decodeProviderMessage(providerMessage)
        : 'The model provider reported that the available quota has been exceeded.',
      details: raw,
    };
  }

  if (isConnectionError(raw)) {
    return {
      title: 'Model connection failed',
      message:
        'Biorouter could not reach the model provider. Check your connection and provider settings, then try again.',
      details: raw,
    };
  }

  return {
    title: isSubmitError ? 'Message could not be sent' : 'Model request failed',
    message: providerMessage
      ? decodeProviderMessage(providerMessage)
      : withoutTransportPrefix || 'The model request failed before it produced a response.',
    details: providerMessage ? raw : undefined,
  };
}

export function ChatTurnError({ error }: { error: string }) {
  const presentation = presentChatTurnError(error);

  return (
    <div data-testid="chat-turn-error" className="mt-4">
      <NotificationSurface
        status="error"
        role="alert"
        title={presentation.title}
        message={presentation.message}
      >
        {presentation.details && presentation.details !== presentation.message && (
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
