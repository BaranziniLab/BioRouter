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
    <div className="pt-4">
      <button
        onClick={toggleExpanded}
        className="w-full flex items-center justify-between py-3 px-2 -mx-2 rounded-lg hover:bg-background-medium transition-colors duration-150 group"
      >
        <h3 className="text-text-default">Conversation Limits</h3>

        <ChevronDown
          className={`w-4 h-4 text-text-muted transition-transform duration-200 ease-in-out ${
            isExpanded ? 'rotate-180' : 'rotate-0'
          }`}
        />
      </button>

      <div
        className={`overflow-hidden transition-all duration-300 ease-in-out ${
          isExpanded ? 'max-h-96 opacity-100 mt-2' : 'max-h-0 opacity-0 mt-0'
        }`}
      >
        <div className="space-y-3 pb-2">
          <div className="flex items-center justify-between py-3 px-2 bg-background-medium rounded-lg">
            <div>
              <h4 className="text-text-default text-sm">Max Turns</h4>
              <p className="text-xs text-text-muted mt-[2px]">
                Maximum agent turns before BioRouter asks for user input
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
