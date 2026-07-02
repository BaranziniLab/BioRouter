import { useEffect, useState, forwardRef } from 'react';
import { Gear } from '../../icons';
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
    description: 'Full file modification capabilities, edit, create, and delete files freely.',
  },
  {
    key: 'approve',
    label: 'Manual',
    description: 'All tools, extensions and file modifications will require human approval',
  },
  {
    key: 'smart_approve',
    label: 'Smart',
    description: 'Intelligently determine which actions need approval based on risk level ',
  },
  {
    key: 'chat',
    label: 'Chat only',
    description: 'Engage with the selected provider without using tools or extensions.',
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
          className={`biorouter-settings-row flex items-center justify-between text-text-default px-3 py-2.5 cursor-pointer ${checked ? 'bg-background-medium/70' : ''}`}
          onClick={() => handleModeChange(mode.key)}
        >
          <div className="flex">
            <div>
              <p className="text-sm font-medium text-text-default">{mode.label}</p>
              {showDescription && (
                <p className="text-xs text-text-muted mt-0.5">{mode.description}</p>
              )}
            </div>
          </div>

          <div className="relative flex items-center gap-2">
            {!isApproveModeConfigure && (mode.key == 'approve' || mode.key == 'smart_approve') && (
              <button
                onClick={(e) => {
                  e.stopPropagation(); // Prevent triggering the mode change
                  setIsPermissionModalOpen(true);
                }}
              >
                <Gear className="w-4 h-4 text-text-muted hover:text-text-default" />
              </button>
            )}
            <input
              type="radio"
              name="modes"
              value={mode.key}
              checked={checked}
              onChange={() => handleModeChange(mode.key)}
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
