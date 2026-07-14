import { useState } from 'react';
import { Button } from '../../../ui/button';
import { SwitchModelModal } from './SwitchModelModal';
import type { View } from '../../../../utils/navigationUtils';
import { shouldShowPredefinedModels } from '../predefinedModelsUtils';

interface ConfigureModelButtonsProps {
  setView: (view: View) => void;
}

export default function ModelSettingsButtons({ setView }: ConfigureModelButtonsProps) {
  const [isAddModelModalOpen, setIsAddModelModalOpen] = useState(false);
  const hasPredefinedModels = shouldShowPredefinedModels();

  return (
    <div className="flex gap-2 pt-4">
      <Button variant="default" onClick={() => setIsAddModelModalOpen(true)}>
        Switch models
      </Button>
      {isAddModelModalOpen ? (
        <SwitchModelModal
          sessionId={null}
          setView={setView}
          onClose={() => setIsAddModelModalOpen(false)}
        />
      ) : null}
      {!hasPredefinedModels && (
        <Button
          variant="secondary"
          onClick={() => {
            setView('ConfigureProviders');
          }}
        >
          Configure providers
        </Button>
      )}
    </div>
  );
}
