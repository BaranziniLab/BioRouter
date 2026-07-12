import { useState } from 'react';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible';
import { ChevronDown, ChevronUp, Loader2 } from './icons/app-icons';
import { Button } from './ui/button';
import { NotificationContent, type NotificationStatus } from './alerts/NotificationSurface';
import { startNewSession } from '../sessions';
import { useNavigation } from '../hooks/useNavigation';
import { formatExtensionErrorMessage } from '../utils/extensionErrorUtils';
import { getInitialWorkingDir } from '../utils/workingDir';
import { formatExtensionName } from './settings/extensions/subcomponents/ExtensionList';

export interface ExtensionLoadingStatus {
  name: string;
  status: 'loading' | 'success' | 'error';
  error?: string;
  recoverHints?: string;
}

interface ExtensionLoadingToastProps {
  extensions: ExtensionLoadingStatus[];
  totalCount: number;
  isComplete: boolean;
}

export function GroupedExtensionLoadingToast({
  extensions,
  isComplete,
}: ExtensionLoadingToastProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [copiedExtension, setCopiedExtension] = useState<string | null>(null);
  const setView = useNavigation();

  const errorCount = extensions.filter((ext) => ext.status === 'error').length;
  const failedNames = extensions
    .filter((ext) => ext.status === 'error')
    .map((ext) => formatExtensionName(ext.name));

  // Per-row markers: 8px status dots (design.md §4.16), tokenised for both themes.
  const getStatusIcon = (status: 'loading' | 'success' | 'error') => {
    switch (status) {
      case 'loading':
        return <Loader2 className="w-3.5 h-3.5 animate-spin text-text-info" />;
      case 'success':
        return <span className="block w-2 h-2 rounded-full bg-background-success" />;
      case 'error':
        return <span className="block w-2 h-2 rounded-full bg-background-danger" />;
    }
  };

  // Summary line. On success we deliberately say "All extensions loaded" rather
  // than a count (many built-ins are now capabilities, so a number is
  // misleading). On failure we name which extensions failed and how many.
  const getSummaryText = () => {
    if (!isComplete) {
      return 'Loading extensions…';
    }
    if (errorCount === 0) {
      return 'All extensions loaded';
    }
    return `${errorCount} extension${errorCount !== 1 ? 's' : ''} failed to load`;
  };

  // Show the per-extension detail / toggle only when something failed — a clean
  // all-loaded toast needs no list (matching the model-change toast's simplicity).
  const showDetails = errorCount > 0;

  // The toast's own status chip + summary now come from the shared primitive
  // (icon: false disables react-toastify's floating glyph). The expandable
  // failure list lives in the primitive's children region.
  const status: NotificationStatus = !isComplete ? 'loading' : errorCount === 0 ? 'success' : 'error';

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen} className="w-full">
      <NotificationContent
        status={status}
        title={getSummaryText()}
        message={errorCount > 0 ? `Failed: ${failedNames.join(', ')}` : undefined}
      >
        {showDetails && (
          <>
            <CollapsibleContent className="overflow-hidden">
              <div className="mt-3 pt-3 border-t border-border-subtle">
                <div className="space-y-3 max-h-64 overflow-y-auto pr-2 [scroll-padding-block:6px]">
                  {extensions.map((ext) => {
                    const friendlyName = formatExtensionName(ext.name);

                    return (
                      <div key={ext.name} className="flex items-start gap-2.5 text-[13px]">
                        <span className="flex-none mt-1">{getStatusIcon(ext.status)}</span>
                        <div className="flex-1 min-w-0">
                          <div className="truncate text-text-default">{friendlyName}</div>
                          {ext.status === 'error' && ext.error && (
                            <>
                              <div className="mt-1 text-xs text-text-muted [overflow-wrap:anywhere]">
                                {formatExtensionErrorMessage(ext.error, 'Failed to add extension')}
                              </div>
                              <div className="mt-1.5 flex flex-wrap gap-2">
                                {ext.recoverHints && setView && (
                                  <Button
                                    size="sm"
                                    variant="secondary"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      startNewSession(
                                        getInitialWorkingDir(),
                                        ext.recoverHints,
                                        setView
                                      );
                                    }}
                                  >
                                    Ask biorouter
                                  </Button>
                                )}
                                <Button
                                  size="sm"
                                  variant="secondary"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    navigator.clipboard.writeText(ext.error!);
                                    setCopiedExtension(ext.name);
                                    setTimeout(() => setCopiedExtension(null), 2000);
                                  }}
                                >
                                  {copiedExtension === ext.name ? 'Copied!' : 'Copy error'}
                                </Button>
                              </div>
                            </>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </CollapsibleContent>

            {/* Toggle button — only when there are failures to inspect */}
            <CollapsibleTrigger asChild>
              <button
                className="mt-2 flex w-full items-center justify-center gap-1 py-1.5 text-xs text-text-subtle transition-colors hover:text-text-default"
                aria-label={isOpen ? 'Collapse details' : 'Expand details'}
              >
                {isOpen ? (
                  <>
                    <span>Show less</span>
                    <ChevronUp className="w-3 h-3" />
                  </>
                ) : (
                  <>
                    <span>Show details</span>
                    <ChevronDown className="w-3 h-3" />
                  </>
                )}
              </button>
            </CollapsibleTrigger>
          </>
        )}
      </NotificationContent>
    </Collapsible>
  );
}
