import {
  UIResourceRenderer,
  UIActionResultIntent,
  UIActionResultLink,
  UIActionResultNotification,
  UIActionResultPrompt,
  UIActionResultToolCall,
  UIActionResult,
} from '@mcp-ui/client';
import { useState, useEffect } from 'react';
import { toast } from 'react-toastify';
import { EmbeddedResource } from '../api';
import { useTheme } from '../contexts/ThemeContext';
import { Maximize2 } from './icons/app-icons';

interface MCPUIResourceRendererProps {
  content: EmbeddedResource & { type: 'resource' };
  appendPromptToChat?: (value: string) => void;
}

// More specific result types using discriminated unions
type UIActionHandlerSuccess<T = unknown> = {
  status: 'success';
  data?: T;
  message?: string;
};

type UIActionHandlerError = {
  status: 'error';
  error: {
    code: UIActionErrorCode;
    message: string;
    details?: unknown;
  };
};

type UIActionHandlerPending = {
  status: 'pending';
  message: string;
};

type UIActionHandlerResult<T = unknown> =
  | UIActionHandlerSuccess<T>
  | UIActionHandlerError
  | UIActionHandlerPending;

// Strongly typed error codes
enum UIActionErrorCode {
  UNSUPPORTED_ACTION = 'UNSUPPORTED_ACTION',
  UNKNOWN_ACTION = 'UNKNOWN_ACTION',
  TOOL_NOT_FOUND = 'TOOL_NOT_FOUND',
  TOOL_EXECUTION_FAILED = 'TOOL_EXECUTION_FAILED',
  NAVIGATION_FAILED = 'NAVIGATION_FAILED',
  PROMPT_FAILED = 'PROMPT_FAILED',
  INTENT_FAILED = 'INTENT_FAILED',
  INVALID_PARAMS = 'INVALID_PARAMS',
  NETWORK_ERROR = 'NETWORK_ERROR',
  TIMEOUT = 'TIMEOUT',
}

// toast component
const ToastComponent = ({
  messageType,
  message,
  isImplemented = true,
}: {
  messageType: string;
  message?: string;
  isImplemented?: boolean;
}) => {
  const title = `MCP-UI ${messageType} message`;

  return (
    <div className="flex flex-col gap-0 py-2 pr-4">
      <p className="font-semibold">{title}</p>
      {isImplemented ? (
        <p>
          Message received for <span className="font-semibold">{message}</span>.
        </p>
      ) : (
        <p>
          Message received for <span className="font-semibold">{message}</span>.
          <br />
          {messageType.charAt(0).toUpperCase() + messageType.slice(1)} messages aren't supported
          yet, refer to console for more details.
        </p>
      )}
    </div>
  );
};

export default function MCPUIResourceRenderer({
  content,
  appendPromptToChat,
}: MCPUIResourceRendererProps) {
  const { resolvedTheme } = useTheme();
  const [proxyUrl, setProxyUrl] = useState<string | undefined>(undefined);

  useEffect(() => {
    const fetchProxyUrl = async () => {
      try {
        const apiHost = await window.electron.getBiorouterdHostPort();
        const secretKey = await window.electron.getSecretKey();
        if (apiHost && secretKey) {
          setProxyUrl(`${apiHost}/mcp-ui-proxy?secret=${encodeURIComponent(secretKey)}`);
        } else {
          console.error('Failed to get biorouterd host/port or secret key');
        }
      } catch (error) {
        console.error('Error fetching MCP-UI Proxy URL:', error);
      }
    };

    fetchProxyUrl().catch(console.error);
  }, []);

  const handleUIAction = async (actionEvent: UIActionResult): Promise<UIActionHandlerResult> => {
    // result to pass back to the MCP-UI
    let result: UIActionHandlerResult;

    const handleToolAction = async (
      actionEvent: UIActionResultToolCall
    ): Promise<UIActionHandlerResult> => {
      const { toolName, params } = actionEvent.payload;
      toast.info(<ToastComponent messageType="tool" message={toolName} isImplemented={false} />, {
        theme: resolvedTheme,
      });
      return {
        status: 'error' as const,
        error: {
          code: UIActionErrorCode.UNSUPPORTED_ACTION,
          message: 'Tool calls are not yet implemented',
          details: { toolName, params },
        },
      };
    };

    const handlePromptAction = async (
      actionEvent: UIActionResultPrompt
    ): Promise<UIActionHandlerResult> => {
      const { prompt } = actionEvent.payload;

      if (appendPromptToChat) {
        try {
          appendPromptToChat(prompt);
          window.dispatchEvent(new CustomEvent('scroll-chat-to-bottom'));
          return {
            status: 'success' as const,
            message: 'Prompt sent to chat successfully',
          };
        } catch (error) {
          return {
            status: 'error' as const,
            error: {
              code: UIActionErrorCode.PROMPT_FAILED,
              message: 'Failed to send prompt to chat',
              details: error instanceof Error ? error.message : error,
            },
          };
        }
      }

      return {
        status: 'error' as const,
        error: {
          code: UIActionErrorCode.UNSUPPORTED_ACTION,
          message: 'Prompt handling is not implemented - append prop is required',
          details: { prompt },
        },
      };
    };

    const handleLinkAction = async (
      actionEvent: UIActionResultLink
    ): Promise<UIActionHandlerResult> => {
      const { url } = actionEvent.payload;

      try {
        const urlObj = new URL(url);
        if (!['http:', 'https:'].includes(urlObj.protocol)) {
          return {
            status: 'error' as const,
            error: {
              code: UIActionErrorCode.NAVIGATION_FAILED,
              message: `Blocked potentially unsafe URL protocol: ${urlObj.protocol}`,
              details: { url, protocol: urlObj.protocol },
            },
          };
        }

        await window.electron.openExternal(url);
        return {
          status: 'success' as const,
          message: `Opened ${url} in default browser`,
        };
      } catch (error) {
        if (error instanceof TypeError && error.message.includes('Invalid URL')) {
          return {
            status: 'error' as const,
            error: {
              code: UIActionErrorCode.INVALID_PARAMS,
              message: `Invalid URL format: ${url}`,
              details: { url, error: error.message },
            },
          };
        } else if (error instanceof Error && error.message.includes('Failed to open')) {
          return {
            status: 'error' as const,
            error: {
              code: UIActionErrorCode.NAVIGATION_FAILED,
              message: `Failed to open URL in default browser`,
              details: { url, error: error.message },
            },
          };
        } else {
          return {
            status: 'error' as const,
            error: {
              code: UIActionErrorCode.NAVIGATION_FAILED,
              message: `Unexpected error opening URL: ${url}`,
              details: error instanceof Error ? error.message : error,
            },
          };
        }
      }
    };

    const handleNotifyAction = async (
      actionEvent: UIActionResultNotification
    ): Promise<UIActionHandlerResult> => {
      const { message } = actionEvent.payload;

      toast.info(<ToastComponent messageType="notify" message={message} isImplemented={true} />, {
        theme: resolvedTheme,
      });
      return {
        status: 'success' as const,
        data: {
          displayedAt: new Date().toISOString(),
          message: 'Notification displayed',
          details: actionEvent.payload,
        },
      };
    };

    const handleIntentAction = async (
      actionEvent: UIActionResultIntent
    ): Promise<UIActionHandlerResult> => {
      toast.info(
        <ToastComponent
          messageType="intent"
          message={actionEvent.payload.intent}
          isImplemented={false}
        />,
        {
          theme: resolvedTheme,
        }
      );
      return {
        status: 'error' as const,
        error: {
          code: UIActionErrorCode.UNSUPPORTED_ACTION,
          message: 'Intent handling is not yet implemented',
          details: actionEvent.payload,
        },
      };
    };

    try {
      switch (actionEvent.type) {
        case 'tool':
          result = await handleToolAction(actionEvent);
          break;

        case 'prompt':
          result = await handlePromptAction(actionEvent);
          break;

        case 'link':
          result = await handleLinkAction(actionEvent);
          break;

        case 'notify':
          result = await handleNotifyAction(actionEvent);
          break;

        case 'intent':
          result = await handleIntentAction(actionEvent);
          break;

        default: {
          const _exhaustiveCheck: never = actionEvent;
          console.error('Unhandled MCP-UI action type:', _exhaustiveCheck);
          result = {
            status: 'error',
            error: {
              code: UIActionErrorCode.UNKNOWN_ACTION,
              message: `Unknown action type`,
              details: actionEvent,
            },
          };
        }
      }
    } catch (error) {
      console.error('Unexpected error handling MCP-UI action:', error);
      result = {
        status: 'error',
        error: {
          code: UIActionErrorCode.UNKNOWN_ACTION,
          message: 'An unexpected error occurred',
          details: error instanceof Error ? error.stack : error,
        },
      };
    }

    return result;
  };

  // Agent-defined preferred frame size (set by the producing tool via
  // `_meta["mcpui.dev/ui-preferred-frame-size"]` = [width, height]).
  const resource = content.resource as {
    uri?: string;
    mimeType?: string;
    blob?: string;
    text?: string;
    _meta?: Record<string, unknown>;
  };
  const prefSize = resource._meta?.['mcpui.dev/ui-preferred-frame-size'] as
    | [string, string]
    | undefined;
  const pxOf = (v?: string): number | undefined => {
    if (!v) return undefined;
    const n = parseInt(v, 10);
    return Number.isFinite(n) && /px$/.test(v) ? n : undefined;
  };
  const prefW = pxOf(prefSize?.[0]);
  const prefH = pxOf(prefSize?.[1]);

  const decodeArtifactHtml = (): string => {
    if (resource.blob) {
      try {
        const bin = atob(resource.blob);
        const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
        return new TextDecoder().decode(bytes);
      } catch {
        return '';
      }
    }
    return resource.text || '';
  };

  const artifactTitle = resource.uri?.split('/').pop() || 'Artifact';

  const handleExpand = async () => {
    const html = decodeArtifactHtml();
    if (!html) {
      toast.error('Could not read the artifact contents.');
      return;
    }
    try {
      await window.electron.openArtifactWindow({
        html,
        title: artifactTitle,
        width: prefW || 1100,
        height: prefH || 820,
        theme: resolvedTheme,
      });
    } catch {
      toast.error('Could not open the artifact window.');
    }
  };

  return (
    <div className="group relative mt-3 overflow-hidden rounded-lg bg-transparent shadow-[0_10px_28px_rgba(15,23,42,0.08)]">
      <div className="absolute right-2 top-2 z-10 opacity-0 transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100">
        <button
          type="button"
          onClick={handleExpand}
          aria-label={`Open ${artifactTitle} in a larger standalone window`}
          title="Open in a larger standalone window"
          className="inline-flex h-7 w-7 items-center justify-center rounded-md border border-border-subtle/70 bg-background-default/85 text-text-muted shadow-sm backdrop-blur transition-colors hover:text-text-default cursor-pointer"
        >
          <Maximize2 className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      </div>
      <div className="overflow-hidden rounded-lg bg-transparent" style={{ minHeight: 320 }}>
        <UIResourceRenderer
          resource={content.resource}
          onUIAction={handleUIAction}
          supportedContentTypes={['rawHtml', 'externalUrl']} // BioRouter does not support remoteDom content
          htmlProps={{
            autoResizeIframe: {
              height: true,
              width: false, // width stays responsive (fills the panel); use Expand for full size
            },
            style: { width: '100%', minHeight: '320px', border: 'none' },
            iframeRenderData: {
              // iframeRenderData allows us to pass data down to MCP-UIs
              // MCP-UIs might find stuff like host and theme for conditional rendering
              // usage of this is experimental, leaving in place for demos
              host: 'biorouter',
              theme: resolvedTheme,
            },
            proxy: proxyUrl, // refer to https://mcpui.dev/guide/client/using-a-proxy
          }}
        />
      </div>
    </div>
  );
}
