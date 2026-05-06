import { useState } from 'react';
import { Button } from '../../ui/button';
import { FolderKey } from '../../icons/app-icons';
import { BioRouterHintsModal } from './BioRouterHintsModal';

export const BioRouterHintsSection = () => {
  const [isModalOpen, setIsModalOpen] = useState(false);
  const directory = window.appConfig?.get('BIOROUTER_WORKING_DIR') as string;

  return (
    <>
      <div className="flex items-center justify-between py-4">
        <div className="flex-1">
          <p className="text-sm font-medium text-text-default">Project Hints (.biorouterhints)</p>
          <p className="text-xs text-text-muted mt-0.5">
            Configure your project's .biorouterhints file to provide additional context to BioRouter
          </p>
        </div>
        <Button
          onClick={() => setIsModalOpen(true)}
          variant="outline"
          className="flex items-center gap-2 ml-4"
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
