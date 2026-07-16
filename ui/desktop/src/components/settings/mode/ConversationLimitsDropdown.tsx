import { useState } from 'react';
import { ChevronDown } from '../../icons/app-icons';
import { Input } from '../../ui/input';

interface ConversationLimitsDropdownProps {
  maxTurns: number;
  onMaxTurnsChange: (value: number) => void;
}

export const ConversationLimitsDropdown = ({
  maxTurns,
  onMaxTurnsChange,
}: ConversationLimitsDropdownProps) => {
  const [isExpanded, setIsExpanded] = useState(false);

  const toggleExpanded = () => {
    setIsExpanded(!isExpanded);
  };

  return (
    <div>
      <button
        onClick={toggleExpanded}
        className="biorouter-settings-row w-full flex items-center justify-between px-3 py-2.5 group"
      >
        <h3 className="text-text-default">Conversation Limits</h3>

        <ChevronDown
          className={`w-4 h-4 text-text-muted transition-transform duration-200 ease-in-out ${
            isExpanded ? 'rotate-180' : 'rotate-0'
          }`}
        />
      </button>

      <div
        className={`overflow-hidden transition-[max-height,opacity] duration-[var(--motion-slow)] ease-[var(--ease-out)] ${
          isExpanded ? 'max-h-96 opacity-100 mt-2' : 'max-h-0 opacity-0 mt-0'
        }`}
      >
        <div className="px-3 pb-3">
          <div className="flex items-center justify-between rounded-lg bg-background-medium/55 px-3 py-2.5">
            <div>
              <h4 className="text-text-default text-sm">Max Turns</h4>
              <p className="text-xs text-text-muted mt-[2px]">
                Maximum agent turns before Biorouter asks for user input
              </p>
            </div>
            <Input
              type="number"
              min="1"
              max="10000"
              value={maxTurns}
              onChange={(e) => onMaxTurnsChange(Number(e.target.value))}
              className="w-20"
            />
          </div>
        </div>
      </div>
    </div>
  );
};
