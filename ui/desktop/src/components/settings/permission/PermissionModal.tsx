import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button } from '../../ui/button';
import { ChevronDownIcon, SlidersHorizontal } from '../../icons/app-icons';
import { getTools, PermissionLevel, ToolInfo, upsertPermissions } from '../../../api';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';
import { toolIdentifierToTitleCase } from '../../../utils';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '../../ui/dropdown-menu';

const permissionOptions = [
  { value: 'always_allow', label: 'Always allow' },
  { value: 'ask_before', label: 'Ask before' },
  { value: 'never_allow', label: 'Never allow' },
] as { value: PermissionLevel; label: string }[];

function getFirstSentence(text: string): string {
  const trimmed = text.trim();
  const match = trimmed.match(/^([^.?!]+[.?!])/);
  return match ? match[0] : trimmed;
}

function getToolLabel(name: string): string {
  const nameParts = name.split('__');
  const toolName = nameParts[nameParts.length - 1] || name;
  return toolIdentifierToTitleCase(toolName);
}

interface PermissionModalProps {
  extensionName: string;
  extensionLabel?: string;
  onClose: () => void;
}

export default function PermissionModal({
  extensionName,
  extensionLabel = extensionName,
  onClose,
}: PermissionModalProps) {
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [updatedPermissions, setUpdatedPermissions] = useState<Record<string, PermissionLevel>>({});
  const [loadStatus, setLoadStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [saveStatus, setSaveStatus] = useState<'idle' | 'saving' | 'error'>('idle');

  const hasChanges = useMemo(
    () =>
      Object.entries(updatedPermissions).some(
        ([toolName, permission]) =>
          permission !== tools.find((tool) => tool.name === toolName)?.permission
      ),
    [updatedPermissions, tools]
  );

  const fetchTools = useCallback(async () => {
    setLoadStatus('loading');
    setTools([]);
    setUpdatedPermissions({});
    try {
      const response = await getTools({
        query: { extension_name: extensionName, session_id: '' },
      });
      if (response.error) {
        setLoadStatus('error');
        return;
      }

      setTools(response.data || []);
      setLoadStatus('ready');
    } catch (error) {
      console.error('Error fetching extension tools:', error);
      setLoadStatus('error');
    }
  }, [extensionName]);

  useEffect(() => {
    void fetchTools();
  }, [fetchTools]);

  const handleSettingChange = (toolName: string, newPermission: PermissionLevel) => {
    setSaveStatus('idle');
    setUpdatedPermissions((previous) => ({
      ...previous,
      [toolName]: newPermission,
    }));
  };

  const handleSave = async () => {
    const toolPermissions = Object.entries(updatedPermissions)
      .filter(
        ([toolName, permission]) =>
          permission !== tools.find((tool) => tool.name === toolName)?.permission
      )
      .map(([toolName, permission]) => ({
        tool_name: toolName,
        permission,
      }));

    if (toolPermissions.length === 0) {
      onClose();
      return;
    }

    setSaveStatus('saving');
    try {
      const response = await upsertPermissions({
        body: { tool_permissions: toolPermissions },
      });
      if (response.error) {
        setSaveStatus('error');
        return;
      }
      onClose();
    } catch (error) {
      console.error('Error saving permissions:', error);
      setSaveStatus('error');
    }
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-h-[min(760px,calc(100vh-2rem))] grid-rows-[auto_minmax(0,1fr)_auto] gap-0 overflow-hidden p-0 sm:max-w-[620px]">
        <DialogHeader className="border-b border-border-subtle px-5 pb-5 pt-5 sm:px-6">
          <div className="flex min-w-0 items-start gap-3 pr-6">
            <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-element bg-background-medium text-text-default">
              <SlidersHorizontal className="h-5 w-5" />
            </div>
            <div className="min-w-0 pt-0.5">
              <DialogTitle className="text-base font-semibold text-text-default break-words [overflow-wrap:anywhere]">
                {extensionLabel}
              </DialogTitle>
              <DialogDescription className="mt-1 text-sm leading-5 text-text-muted">
                Override how this extension’s tools behave in Manual and Smart modes.
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="min-h-0 overflow-y-auto px-5 py-4 sm:px-6">
          {loadStatus === 'loading' && (
            <div className="flex min-h-40 items-center justify-center text-sm text-text-muted">
              Loading tools…
            </div>
          )}

          {loadStatus === 'error' && (
            <div className="flex min-h-40 flex-col items-center justify-center gap-3 text-center">
              <div>
                <p className="text-sm font-medium text-text-default">Tools could not be loaded</p>
                <p className="mt-1 max-w-sm text-sm leading-5 text-text-muted">
                  Check that the extension is installed and can start, then try again.
                </p>
              </div>
              <Button variant="outline" size="sm" onClick={fetchTools}>
                Try again
              </Button>
            </div>
          )}

          {loadStatus === 'ready' && tools.length === 0 && (
            <div className="flex min-h-40 flex-col items-center justify-center text-center">
              <p className="text-sm font-medium text-text-default">No configurable tools</p>
              <p className="mt-1 max-w-sm text-sm leading-5 text-text-muted">
                This extension loaded, but it exposes no tools.
              </p>
            </div>
          )}

          {loadStatus === 'ready' && tools.length > 0 && (
            <div className="biorouter-settings-list">
              {tools.map((tool) => {
                const selectedPermission = updatedPermissions[tool.name] || tool.permission;
                const selectedLabel =
                  permissionOptions.find((option) => option.value === selectedPermission)?.label ||
                  'Ask before';

                return (
                  <div
                    key={tool.name}
                    className="biorouter-settings-row flex min-w-0 flex-col gap-3 px-3 py-3 sm:flex-row sm:items-center sm:justify-between"
                  >
                    <div className="min-w-0 flex-1">
                      <p
                        className="text-sm font-medium text-text-default break-words [overflow-wrap:anywhere]"
                        title={tool.name}
                      >
                        {getToolLabel(tool.name)}
                      </p>
                      {tool.description && (
                        <p className="mt-0.5 text-xs leading-5 text-text-muted break-words [overflow-wrap:anywhere]">
                          {getFirstSentence(tool.description)}
                        </p>
                      )}
                    </div>
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          className="w-full flex-shrink-0 justify-between sm:w-36"
                          variant="secondary"
                          size="sm"
                        >
                          {selectedLabel}
                          <ChevronDownIcon className="h-4 w-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="min-w-36">
                        {permissionOptions.map((option) => (
                          <DropdownMenuItem
                            key={option.value}
                            onSelect={() => handleSettingChange(tool.name, option.value)}
                          >
                            {option.label}
                          </DropdownMenuItem>
                        ))}
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <DialogFooter className="border-t border-border-subtle px-5 py-4 sm:px-6">
          {saveStatus === 'error' && (
            <p className="mr-auto self-center text-sm text-text-danger">
              Permissions could not be saved.
            </p>
          )}
          <Button variant="outline" onClick={onClose} disabled={saveStatus === 'saving'}>
            Cancel
          </Button>
          <Button
            disabled={!hasChanges || loadStatus !== 'ready' || saveStatus === 'saving'}
            onClick={handleSave}
          >
            {saveStatus === 'saving' ? 'Saving…' : 'Save changes'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
