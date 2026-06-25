import { useState } from 'react';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible';
import { ChevronDown, ChevronUp, Loader2 } from './icons/app-icons';
import { Button } from './ui/button';
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

  const getStatusIcon = (status: 'loading' | 'success' | 'error') => {
    switch (status) {
      case 'loading':
        return <Loader2 className="w-4 h-4 animate-spin text-text-info" />;
      case 'success':
        return <div className="w-4 h-4 rounded-full bg-background-success" />;
      case 'error':
        return <div className="w-4 h-4 rounded-full bg-background-danger" />;
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

  return (
    <div className="w-full">
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <div className="flex flex-col">
          {/* Main summary section — typography matches the standard toast
              (toastSuccess/toastError): a bold font-medium title + a plain msg
              line. The status icon/theme is supplied by the react-toastify toast
              type set in toastService.extensionLoading. */}
          <CollapsibleTrigger asChild>
            <div
              className={`flex items-start gap-3 pr-8 ${showDetails ? 'cursor-pointer hover:opacity-90 transition-opacity' : ''}`}
            >
              <div className="flex-1 min-w-0">
                <strong className="font-medium">{getSummaryText()}</strong>
                {errorCount > 0 && (
                  <div className="text-sm opacity-90">Failed: {failedNames.join(', ')}</div>
                )}
              </div>
            </div>
          </CollapsibleTrigger>

          {/* Expanded details section */}
          <CollapsibleContent className="overflow-hidden">
            <div className="mt-3 pt-3 border-t border-white/20">
              <div className="space-y-3 max-h-64 overflow-y-auto pr-2 pl-1">
                {extensions.map((ext) => {
                  const friendlyName = formatExtensionName(ext.name);

                  return (
                    <div key={ext.name} className="flex flex-col gap-2">
                      <div className="flex items-center gap-3 text-sm">
                        {getStatusIcon(ext.status)}
                        <div className="flex-1 min-w-0 truncate">{friendlyName}</div>
                      </div>
                      {ext.status === 'error' && ext.error && (
                        <div className="ml-7 flex flex-col gap-2">
                          <div className="text-xs opacity-75 break-words">
                            {formatExtensionErrorMessage(ext.error, 'Failed to add extension')}
                          </div>
                          <div className="flex gap-2">
                            {ext.recoverHints && setView && (
                              <Button
                                size="sm"
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
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          </CollapsibleContent>

          {/* Toggle button — only when there are failures to inspect */}
          {showDetails && (
            <CollapsibleTrigger asChild>
              <button
                className="flex items-center justify-center gap-1 text-xs opacity-60 hover:opacity-100 transition-opacity mt-2 py-1.5 w-full"
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
          )}
        </div>
      </Collapsible>
    </div>
  );
}
