import { useState, useEffect } from 'react';
import { Switch } from '../../ui/switch';

export const SpellcheckToggle = () => {
  const [enabled, setEnabled] = useState(true);

  useEffect(() => {
    const loadState = async () => {
      const state = await window.electron.getSpellcheckState();
      setEnabled(state);
    };
    loadState();
  }, []);

  const handleToggle = async (checked: boolean) => {
    setEnabled(checked);
    await window.electron.setSpellcheck(checked);
  };

  return (
    <div className="biorouter-settings-row flex items-center justify-between px-3 py-2.5">
      <div>
        <p className="text-sm font-medium text-text-default">Enable Spellcheck</p>
        <p className="text-xs text-text-muted mt-0.5 max-w-md">
          Check spelling in the chat input. Requires restart to take effect.
        </p>
      </div>
      <Switch checked={enabled} onCheckedChange={handleToggle} variant="mono" />
    </div>
  );
};
