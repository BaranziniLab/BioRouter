import { useEffect, useState, forwardRef } from 'react';
import { SlidersHorizontal } from '../../icons/app-icons';
import { Button } from '../../ui/button';
import { ConfigureApproveMode } from './ConfigureApproveMode';
import PermissionRulesModal from '../permission/PermissionRulesModal';

export interface BioRouterMode {
  key: string;
  label: string;
  description: string;
}

export const all_biorouter_modes: BioRouterMode[] = [
  {
    key: 'auto',
    label: 'Autonomous',
    description: 'Use tools and edit, create, or delete files without asking first.',
  },
  {
    key: 'approve',
    label: 'Manual',
    description: 'Ask before using tools, extensions, or making file changes.',
  },
  {
    key: 'smart_approve',
    label: 'Smart',
    description: 'Ask only when an action’s risk level requires your approval.',
  },
  {
    key: 'chat',
    label: 'Chat only',
    description: 'Chat with the selected model without tools or extensions.',
  },
];

interface ModeSelectionItemProps {
  currentMode: string;
  mode: BioRouterMode;
  showDescription: boolean;
  isApproveModeConfigure: boolean;
  handleModeChange: (newMode: string) => void;
}

export const ModeSelectionItem = forwardRef<HTMLDivElement, ModeSelectionItemProps>(
  ({ currentMode, mode, showDescription, isApproveModeConfigure, handleModeChange }, ref) => {
    const [checked, setChecked] = useState(currentMode == mode.key);
    const [isDialogOpen, setIsDialogOpen] = useState(false);
    const [isPermissionModalOpen, setIsPermissionModalOpen] = useState(false);

    useEffect(() => {
      setChecked(currentMode === mode.key);
    }, [currentMode, mode.key]);

    return (
      <div ref={ref} className="group hover:cursor-pointer text-sm">
        <div
          className={`biorouter-settings-row flex min-w-0 items-center justify-between gap-3 px-3 py-2.5 text-text-default cursor-pointer ${checked ? 'bg-background-medium/70' : ''}`}
          onClick={() => handleModeChange(mode.key)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault();
              handleModeChange(mode.key);
            }
          }}
          role="radio"
          aria-checked={checked}
          tabIndex={0}
        >
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-text-default break-words">{mode.label}</p>
            {showDescription && (
              <p className="mt-0.5 text-xs leading-5 text-text-muted break-words [overflow-wrap:anywhere]">
                {mode.description}
              </p>
            )}
          </div>

          <div className="relative flex flex-shrink-0 items-center gap-2">
            {!isApproveModeConfigure && (mode.key == 'approve' || mode.key == 'smart_approve') && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="px-2 text-xs text-text-muted hover:text-text-default"
                onClick={(e) => {
                  e.stopPropagation();
                  setIsPermissionModalOpen(true);
                }}
                aria-label={`Configure ${mode.label} tool permissions`}
              >
                <SlidersHorizontal className="h-4 w-4" />
                <span className="hidden sm:inline">Permissions</span>
              </Button>
            )}
            <input
              type="radio"
              name="modes"
              value={mode.key}
              checked={checked}
              onChange={() => handleModeChange(mode.key)}
              aria-hidden="true"
              tabIndex={-1}
              className="peer sr-only"
            />
            <div
              className="h-4 w-4 rounded-full border border-text-muted
                    peer-checked:border-[6px] peer-checked:border-text-default
                    peer-checked:bg-background-default
                    transition-all duration-200 ease-in-out group-hover:border-text-default"
            ></div>
          </div>
        </div>
        <div>
          <div>
            {isDialogOpen ? (
              <ConfigureApproveMode
                onClose={() => {
                  setIsDialogOpen(false);
                }}
                handleModeChange={handleModeChange}
                currentMode={currentMode}
              />
            ) : null}
          </div>
        </div>

        <PermissionRulesModal
          isOpen={isPermissionModalOpen}
          onClose={() => setIsPermissionModalOpen(false)}
        />
      </div>
    );
  }
);

ModeSelectionItem.displayName = 'ModeSelectionItem';
