import { useState, useEffect } from 'react';
import { toolIdentifierToTitleCase } from '../utils';
import PermissionModal from './settings/permission/PermissionModal';
import { ChevronRight, Lock, Check, X, AlertTriangle } from './icons/app-icons';
import { confirmToolAction, ActionRequired } from '../api';
import { Button } from './ui/button';
import { ToolCallPreview, ToolRiskBadge } from './ToolCallPreview';
import { userActionHeaders } from '../utils/userAction';

const ALLOW_ONCE = 'allow_once';
const ALWAYS_ALLOW = 'always_allow';
const DENY = 'deny';

// Global state to track tool confirmation decisions
// This persists across navigation within the same session
const toolConfirmationState = new Map<
  string,
  {
    clicked: boolean;
    status: string;
    actionDisplay: string;
  }
>();

type ToolConfirmationData = Extract<ActionRequired['data'], { actionType: 'toolConfirmation' }>;

/** `developer__text_editor` → `Text Editor`. */
function friendlyToolName(toolName: string): string {
  return toolIdentifierToTitleCase(toolName.substring(toolName.lastIndexOf('__') + 2));
}

interface ToolConfirmationProps {
  sessionId: string;
  isCancelledMessage: boolean;
  isClicked: boolean;
  actionRequiredContent: ActionRequired & { type: 'actionRequired' };
}

export default function ToolConfirmation({
  sessionId,
  isCancelledMessage,
  isClicked,
  actionRequiredContent,
}: ToolConfirmationProps) {
  const data = actionRequiredContent.data as ToolConfirmationData;
  // BR-63: `risk` and `preview` are optional — a confirmation persisted before
  // BR-63, or replayed from an older daemon, simply has neither and the card
  // degrades to its pre-BR-63 shape rather than breaking.
  const { id: toolConfirmationId, toolName, prompt, risk, preview } = data;

  // Check if we have a stored state for this tool confirmation
  const storedState = toolConfirmationState.get(toolConfirmationId);

  // Initialize state from stored state if available, otherwise use props/defaults
  const [clicked, setClicked] = useState(storedState?.clicked ?? isClicked);
  const [status, setStatus] = useState(storedState?.status ?? 'unknown');
  const [actionDisplay, setActionDisplay] = useState(storedState?.actionDisplay ?? '');
  const [isModalOpen, setIsModalOpen] = useState(false);

  // Sync internal state with stored state and props
  useEffect(() => {
    const currentStoredState = toolConfirmationState.get(toolConfirmationId);

    // If we have stored state, use it
    if (currentStoredState) {
      setClicked(currentStoredState.clicked);
      setStatus(currentStoredState.status);
      setActionDisplay(currentStoredState.actionDisplay);
    } else if (isClicked && !clicked) {
      // Fallback to prop-based logic for historical confirmations
      setClicked(isClicked);
      if (status === 'unknown') {
        setStatus('confirmed');
        setActionDisplay('confirmed');

        // Store this state for future renders
        toolConfirmationState.set(toolConfirmationId, {
          clicked: true,
          status: 'confirmed',
          actionDisplay: 'confirmed',
        });
      }
    }
  }, [isClicked, clicked, status, toolName, toolConfirmationId]);

  const handleButtonClick = async (newStatus: string) => {
    let newActionDisplay;

    if (newStatus === ALWAYS_ALLOW) {
      newActionDisplay = 'always allowed';
    } else if (newStatus === ALLOW_ONCE) {
      newActionDisplay = 'allowed once';
    } else if (newStatus === DENY) {
      newActionDisplay = 'denied';
    } else {
      newActionDisplay = 'denied';
    }

    // Update local state
    setClicked(true);
    setStatus(newStatus);
    setActionDisplay(newActionDisplay);

    // Store in global state for persistence across navigation
    toolConfirmationState.set(toolConfirmationId, {
      clicked: true,
      status: newStatus,
      actionDisplay: newActionDisplay,
    });

    try {
      const response = await confirmToolAction({
        headers: await userActionHeaders(),
        body: {
          sessionId: sessionId,
          id: toolConfirmationId,
          action: newStatus,
          principalType: 'Tool',
        },
      });
      if (response.error) {
        console.error('Failed to confirm tool action:', response.error);
      }
    } catch (err) {
      console.error('Error confirming tool action:', err);
    }
  };

  const handleModalClose = () => {
    setIsModalOpen(false);
  };

  function getExtensionName(toolName: string): string {
    const parts = toolName.split('__');
    return parts.length > 1 ? parts[0] : '';
  }

  // One cohesive, bordered "permission request" card. A single border wraps the
  // whole element (header + actions) so there are no mismatched borders, it uses
  // the app's standard card tokens + typography, and a gentle slide-in makes it
  // read as a distinct prompt the user is meant to act on.
  return isCancelledMessage ? (
    <div className="biorouter-message-content rounded-2xl border border-border-subtle bg-background-muted px-4 py-3 text-sm text-text-muted">
      Tool call confirmation was cancelled.
    </div>
  ) : (
    <>
      <div className="biorouter-message-content overflow-hidden rounded-2xl border border-border-subtle bg-background-default animate-in fade-in slide-in-from-bottom-1 duration-200">
        {/* Security finding banner, only when the backend flagged one */}
        {prompt && (
          <div className="flex items-start gap-2 border-b border-border-subtle bg-background-warning/10 px-4 py-2.5 text-sm text-text-warning">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <span>{prompt}</span>
          </div>
        )}

        {clicked ? (
          // Resolved state — one consistent row inside the same card.
          <div className="flex items-center justify-between px-4 py-3">
            <div className="flex items-center gap-2 text-sm text-text-default">
              {status === 'deny' ? (
                <X className="h-4 w-4 shrink-0 text-text-muted" />
              ) : (
                <Check className="h-4 w-4 shrink-0 text-text-muted" />
              )}
              <span>
                {isClicked
                  ? 'Tool confirmation is not available'
                  : `${friendlyToolName(toolName)} is ${actionDisplay}`}
              </span>
            </div>
            <button
              type="button"
              className="flex items-center gap-1 text-sm text-text-muted transition-colors hover:text-text-default"
              onClick={() => setIsModalOpen(true)}
            >
              Change
              <ChevronRight className="h-4 w-4" />
            </button>
          </div>
        ) : (
          // Pending state — the agent is asking the user for permission.
          <div className="px-4 py-3">
            <div className="mb-3 flex flex-wrap items-center gap-2">
              <Lock className="h-4 w-4 shrink-0 text-text-muted" />
              <span className="text-sm font-medium text-text-default">
                {/* Name the tool. "this tool" told the user nothing. */}
                Run <span className="font-mono">{friendlyToolName(toolName)}</span>?
              </span>
              {risk && <ToolRiskBadge risk={risk} />}
            </div>

            {/* BR-63: the whole point — what the call will actually do. */}
            {preview && (
              <div className="mb-3">
                <ToolCallPreview preview={preview} />
              </div>
            )}

            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm" variant="default" onClick={() => handleButtonClick(ALLOW_ONCE)}>
                Allow Once
              </Button>
              {/* Only offer "Always Allow" when there's no security finding. */}
              {!prompt && (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => handleButtonClick(ALWAYS_ALLOW)}
                >
                  Always Allow
                </Button>
              )}
              <Button size="sm" variant="outline" onClick={() => handleButtonClick(DENY)}>
                Deny
              </Button>
            </div>
          </div>
        )}
      </div>

      {/* Modal for updating tool permission */}
      {isModalOpen && (
        <PermissionModal onClose={handleModalClose} extensionName={getExtensionName(toolName)} />
      )}
    </>
  );
}
