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
import { EmbeddedResource } from '../api';
import { useTheme } from '../contexts/ThemeContext';
import { toastError, toastInfo } from '../toasts';
import { injectArtifactBrowserCsp } from '../utils/artifactSecurity';
import { ExternalLink, Maximize2 } from './icons/app-icons';
import type { ArtifactSource } from './artifacts/artifactTypes';
import { artifactSourceFromResource, titleFromResourceUri } from './artifacts/artifactUtils';

const MAX_UI_PROMPT_BYTES = 64 * 1024;

interface MCPUIResourceRendererProps {
  content: EmbeddedResource & { type: 'resource' };
  /** The chat this resource is rendered inside. Used to scope the
   * 'scroll-chat-to-bottom' broadcast so a prompt action here doesn't scroll
   * every other mounted chat. */
  sessionId?: string | null;
  appendPromptToChat?: (value: string) => void;
  onOpenArtifact?: (artifact: ArtifactSource) => void;
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

export default function MCPUIResourceRenderer({
  content,
  sessionId,
  appendPromptToChat,
  onOpenArtifact,
}: MCPUIResourceRendererProps) {
  // This component's own chrome (the artifact card, the pending-link and
  // pending-prompt bars, the expand button) carries no colour literals — it is
  // styled entirely in semantic Tailwind classes (`bg-background-default`,
  // `text-text-muted`, `border-border-subtle`), which are CSS custom properties
  // that main.css already re-points per `[data-theme]`. So the chrome is
  // family-aware for free. What is NOT is the guest document inside the
  // `UIResourceRenderer` iframe — see `iframeRenderData` below.
  const { resolvedTheme, themeFamily } = useTheme();
  const [proxyUrl, setProxyUrl] = useState<string | undefined>(undefined);
  const [pendingExternalUrl, setPendingExternalUrl] = useState<string | null>(null);
  const [pendingPrompt, setPendingPrompt] = useState<string | null>(null);

  useEffect(() => {
    const fetchProxyUrl = async () => {
      try {
        const apiHost = await window.electron.getBiorouterdHostPort();
        if (apiHost) {
          setProxyUrl(`${apiHost}/mcp-ui-proxy`);
        } else {
          console.error('Failed to get biorouterd host/port');
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
      toastInfo({
        title: 'MCP-UI tool message',
        msg: `Message received for ${toolName}. Tool messages aren't supported yet; refer to the console for more details.`,
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
        if (
          typeof prompt !== 'string' ||
          new TextEncoder().encode(prompt).byteLength > MAX_UI_PROMPT_BYTES
        ) {
          return {
            status: 'error' as const,
            error: {
              code: UIActionErrorCode.INVALID_PARAMS,
              message: 'Prompt is invalid or too large',
            },
          };
        }
        setPendingPrompt(prompt);
        return {
          status: 'pending' as const,
          message: 'Waiting for the user to send the prompt',
        };
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
        if (url.length > 8 * 1024) {
          throw new TypeError('Invalid URL: value is too long');
        }
        const urlObj = new URL(url);
        if (
          !['http:', 'https:'].includes(urlObj.protocol) ||
          urlObj.username !== '' ||
          urlObj.password !== ''
        ) {
          return {
            status: 'error' as const,
            error: {
              code: UIActionErrorCode.NAVIGATION_FAILED,
              message: `Blocked potentially unsafe URL protocol: ${urlObj.protocol}`,
              details: { url, protocol: urlObj.protocol },
            },
          };
        }

        setPendingExternalUrl(urlObj.toString());
        return {
          status: 'pending' as const,
          message: 'Waiting for the user to open the link',
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

      toastInfo({
        title: 'MCP-UI notification message',
        msg: `Message received for ${message}.`,
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
      toastInfo({
        title: 'MCP-UI intent message',
        msg: `Message received for ${actionEvent.payload.intent}. Intent messages aren't supported yet; refer to the console for more details.`,
      });
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

  const fallbackArtifactTitle =
    titleFromResourceUri(resource.uri) || resource.uri?.split('/').pop() || 'Artifact';
  const artifactSource = artifactSourceFromResource(content, fallbackArtifactTitle);
  const artifactTitle = artifactSource?.title ?? fallbackArtifactTitle;

  const handleOpenArtifact = async () => {
    if (artifactSource && onOpenArtifact) {
      onOpenArtifact(artifactSource);
      return;
    }
    if (artifactSource?.kind === 'externalUrl') {
      await window.electron.openExternal(artifactSource.url);
      return;
    }
    if (artifactSource?.kind !== 'html') {
      toastError({
        title: 'Artifact unavailable',
        msg: 'This resource does not provide a browser-safe HTML preview.',
      });
      return;
    }
    try {
      await window.electron.openArtifactWindow({
        html: artifactSource.html,
        title: artifactTitle,
        width: prefW || 1100,
        height: prefH || 820,
        theme: resolvedTheme,
      });
    } catch {
      toastError({
        title: 'Artifact unavailable',
        msg: 'Could not open the artifact window.',
      });
    }
  };

  if (artifactSource && (onOpenArtifact || artifactSource.kind === 'externalUrl')) {
    const destination =
      artifactSource.kind === 'externalUrl' ? 'in the default browser' : 'in the artifact viewer';
    return (
      <div className="group mt-3 flex w-full max-w-xl items-center gap-2">
        <button
          type="button"
          onClick={handleOpenArtifact}
          aria-label={`Open ${artifactTitle} ${destination}`}
          className="flex min-w-0 flex-1 items-center gap-3 rounded-lg border border-border-subtle bg-background-default/75 px-3 py-2.5 text-left shadow-popover transition-colors hover:bg-background-medium"
        >
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-background-medium text-text-muted">
            <Maximize2 className="h-4 w-4" aria-hidden="true" />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm font-medium text-text-default">
              {artifactTitle}
            </span>
            <span className="block truncate text-xs text-text-muted">
              {resource.mimeType || 'text/html'}
            </span>
          </span>
        </button>
      </div>
    );
  }

  if (artifactSource?.kind !== 'html') {
    return (
      <div className="mt-3 text-xs text-text-muted" role="status">
        No browser-safe preview is available for {artifactTitle}.
      </div>
    );
  }

  const securedResource = {
    ...content.resource,
    blob: undefined,
    text: injectArtifactBrowserCsp(artifactSource.html),
    mimeType: 'text/html',
  };

  return (
    <div className="group relative mt-3 overflow-hidden rounded-lg bg-transparent shadow-popover">
      {pendingExternalUrl && (
        <div className="flex items-center gap-2 border-b border-border-subtle bg-background-default px-3 py-2 text-xs">
          <span className="min-w-0 flex-1 truncate text-text-muted">{pendingExternalUrl}</span>
          <button
            type="button"
            onClick={() => {
              void window.electron.openExternal(pendingExternalUrl);
              setPendingExternalUrl(null);
            }}
            className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-border-subtle px-2 font-medium text-text-default hover:bg-background-medium"
          >
            <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
            Open link
          </button>
          <button
            type="button"
            onClick={() => setPendingExternalUrl(null)}
            className="h-7 shrink-0 rounded-md px-2 text-text-muted hover:bg-background-medium hover:text-text-default"
          >
            Dismiss
          </button>
        </div>
      )}
      {pendingPrompt !== null && (
        <div className="flex items-center gap-2 border-b border-border-subtle bg-background-default px-3 py-2 text-xs">
          <span className="min-w-0 flex-1 truncate text-text-muted">
            Artifact prompt: {pendingPrompt}
          </span>
          <button
            type="button"
            onClick={() => {
              try {
                appendPromptToChat?.(pendingPrompt);
                window.dispatchEvent(
                  new CustomEvent('scroll-chat-to-bottom', { detail: { sessionId } })
                );
                setPendingPrompt(null);
              } catch (error) {
                toastError({
                  title: 'Could not send artifact prompt',
                  msg: error instanceof Error ? error.message : String(error),
                });
              }
            }}
            className="inline-flex h-7 shrink-0 items-center rounded-md border border-border-subtle px-2 font-medium text-text-default hover:bg-background-medium"
          >
            Send prompt
          </button>
          <button
            type="button"
            onClick={() => setPendingPrompt(null)}
            className="h-7 shrink-0 rounded-md px-2 text-text-muted hover:bg-background-medium hover:text-text-default"
          >
            Dismiss
          </button>
        </div>
      )}
      <div className="absolute right-2 top-2 z-10 flex gap-1 opacity-0 transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100">
        <button
          type="button"
          onClick={handleOpenArtifact}
          aria-label={`Open ${artifactTitle} ${
            onOpenArtifact ? 'in the artifact viewer' : 'in a larger standalone window'
          }`}
          title={onOpenArtifact ? 'Open in artifact viewer' : 'Open in a larger standalone window'}
          className="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-md border border-border-subtle bg-background-default/85 text-text-muted transition-colors hover:text-text-default"
        >
          <Maximize2 className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      </div>
      <div className="overflow-hidden rounded-lg bg-transparent" style={{ minHeight: 320 }}>
        <UIResourceRenderer
          resource={securedResource}
          onUIAction={handleUIAction}
          supportedContentTypes={['rawHtml', 'externalUrl']} // Biorouter does not support remoteDom content
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
              // The other half of the same fact. `theme` alone tells a guest
              // light vs dark but not WHICH light — Parchment's warm white,
              // Alma Mater's navy-inked white, or Roche Limit's neutral — so a
              // guest that tried to blend with its host could only ever match
              // Parchment. This is a declaration for guests that choose to read
              // it (the field is a host-metadata bag, which is exactly what a
              // family id is), not a fix to our own figures: the Auto Visualiser
              // HTML is generated in `crates/` and currently resolves light/dark
              // only, so it ignores this today. Costs nothing, weakens no CSP,
              // and means a family-aware guest is possible without another
              // protocol change.
              themeFamily,
            },
            iframeProps: {
              // @ts-expect-error -- @mcp-ui/client narrows iframeProps to generic HTMLAttributes.
              name: 'biorouter-artifact-preview',
            },
            proxy: proxyUrl, // refer to https://mcpui.dev/guide/client/using-a-proxy
          }}
        />
      </div>
    </div>
  );
}
