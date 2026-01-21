import { useState } from 'react';
import { Button } from '../../ui/button';
import { FolderKey } from 'lucide-react';
import { BioRouterHintsModal } from './BioRouterHintsModal';

export const BioRouterHintsSection = () => {
  const [isModalOpen, setIsModalOpen] = useState(false);
  const directory = window.appConfig?.get('BIOROUTER_WORKING_DIR') as string;

  return (
    <>
      <div className="flex items-center justify-between px-2 py-2">
        <div className="flex-1">
          <h3 className="text-text-default">Project Hints (.biorouterhints)</h3>
          <p className="text-xs text-text-muted mt-[2px]">
            Configure your project's .biorouterhints file to provide additional context to BioRouter
          </p>
        </div>
        <Button
          onClick={() => setIsModalOpen(true)}
          variant="outline"
          size="sm"
          className="flex items-center gap-2"
        >
          <FolderKey size={16} />
          Configure
        </Button>
      </div>
      {isModalOpen && (
        <BioRouterHintsModal directory={directory} setIsBioRouterHintsModalOpen={setIsModalOpen} />
      )}
    </>
  );
};
