import { useCallback, useEffect, useState } from 'react';
import { Tornado } from '../icons/app-icons';
import { all_biorouter_modes, ModeSelectionItem } from '../settings/mode/ModeSelectionItem';
import { useConfig } from '../ConfigContext';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';

export const BottomMenuModeSelection = () => {
  const [currentMode, setCurrentMode] = useState('auto');
  const { read, upsert } = useConfig();

  const fetchCurrentMode = useCallback(async () => {
    try {
      const mode = (await read('BIOROUTER_MODE', false)) as string;
      if (mode) {
        setCurrentMode(mode);
      }
    } catch (error) {
      console.error('Error fetching current mode:', error);
    }
  }, [read]);

  useEffect(() => {
    fetchCurrentMode();
  }, [fetchCurrentMode]);

  const handleModeChange = async (newMode: string) => {
    if (currentMode === newMode) {
      return;
    }

    try {
      await upsert('BIOROUTER_MODE', newMode, false);
      setCurrentMode(newMode);
    } catch (error) {
      console.error('Error updating biorouter mode:', error);
      throw new Error(`Failed to store new biorouter mode: ${newMode}`);
    }
  };

  function getValueByKey(key: string) {
    const mode = all_biorouter_modes.find((mode) => mode.key === key);
    return mode ? mode.label : 'auto';
  }

  function getModeDescription(key: string) {
    const mode = all_biorouter_modes.find((mode) => mode.key === key);
    return mode ? mode.description : 'Automatic mode selection';
  }

  return (
    <div title={`Current mode: ${getValueByKey(currentMode)} - ${getModeDescription(currentMode)}`}>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <span className="flex items-center cursor-pointer [&_svg]:size-4 text-text-default/70 hover:text-text-default hover:scale-100 hover:bg-transparent text-xs">
            <Tornado className="mr-1 h-4 w-4" />
            {getValueByKey(currentMode).toLowerCase()}
          </span>
        </DropdownMenuTrigger>
        <DropdownMenuContent className="w-64" side="top" align="center">
          {all_biorouter_modes.map((mode) => (
            <DropdownMenuItem key={mode.key} asChild>
              <ModeSelectionItem
                mode={mode}
                currentMode={currentMode}
                showDescription={false}
                isApproveModeConfigure={false}
                handleModeChange={handleModeChange}
              />
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
};
