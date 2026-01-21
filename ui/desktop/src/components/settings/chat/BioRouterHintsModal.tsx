import { useState, useEffect } from 'react';
import { Button } from '../../ui/button';
import { Check } from '../../icons';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';

const HelpText = () => (
  <div className="text-sm flex-col space-y-4 text-textSubtle">
    <p>
      .biorouterhints is a text file used to provide additional context about your project and improve
      the communication with BioRouter.
    </p>
    <p>
      Please make sure <span className="font-bold">Developer</span> extension is enabled in the
      extensions page. This extension is required to use .biorouterhints. You'll need to restart your
      session for .biorouterhints updates to take effect.
    </p>
    <p>
      See{' '}
      <Button
        variant="link"
        className="text-blue-500 hover:text-blue-600 p-0 h-auto"
        onClick={() =>
          window.open('https://github.com/BaranziniLab/BioRouter/docs/guides/using-biorouterhints/', '_blank')
        }
      >
        using .biorouterhints
      </Button>{' '}
      for more information.
    </p>
  </div>
);

const ErrorDisplay = ({ error }: { error: Error }) => (
  <div className="text-sm text-textSubtle">
    <div className="text-red-600">Error reading .biorouterhints file: {JSON.stringify(error)}</div>
  </div>
);

const FileInfo = ({ filePath, found }: { filePath: string; found: boolean }) => (
  <div className="text-sm font-medium mb-2">
    {found ? (
      <div className="text-green-600">
        <Check className="w-4 h-4 inline-block" /> .biorouterhints file found at: {filePath}
      </div>
    ) : (
      <div>Creating new .biorouterhints file at: {filePath}</div>
    )}
  </div>
);

const getBioRouterHintsFile = async (filePath: string) => await window.electron.readFile(filePath);

interface BioRouterHintsModalProps {
  directory: string;
  setIsBioRouterHintsModalOpen: (isOpen: boolean) => void;
}

export const BioRouterHintsModal = ({ directory, setIsBioRouterHintsModalOpen }: BioRouterHintsModalProps) => {
  const biorouterHintsFilePath = `${directory}/.biorouterhints`;
  const [biorouterHintsFile, setBioRouterHintsFile] = useState<string>('');
  const [biorouterHintsFileFound, setBioRouterHintsFileFound] = useState<boolean>(false);
  const [biorouterHintsFileReadError, setBioRouterHintsFileReadError] = useState<string>('');
  const [isSaving, setIsSaving] = useState(false);
  const [saveSuccess, setSaveSuccess] = useState(false);

  useEffect(() => {
    const fetchBioRouterHintsFile = async () => {
      try {
        const { file, error, found } = await getBioRouterHintsFile(biorouterHintsFilePath);
        setBioRouterHintsFile(file);
        setBioRouterHintsFileFound(found);
        setBioRouterHintsFileReadError(found && error ? error : '');
      } catch (error) {
        console.error('Error fetching .biorouterhints file:', error);
        setBioRouterHintsFileReadError('Failed to access .biorouterhints file');
      }
    };
    if (directory) fetchBioRouterHintsFile();
  }, [directory, biorouterHintsFilePath]);

  const writeFile = async () => {
    setIsSaving(true);
    setSaveSuccess(false);
    try {
      await window.electron.writeFile(biorouterHintsFilePath, biorouterHintsFile);
      setSaveSuccess(true);
      setBioRouterHintsFileFound(true);
      setTimeout(() => setSaveSuccess(false), 3000);
    } catch (error) {
      console.error('Error writing .biorouterhints file:', error);
      setBioRouterHintsFileReadError('Failed to save .biorouterhints file');
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open={true} onOpenChange={(open) => setIsBioRouterHintsModalOpen(open)}>
      <DialogContent className="w-[80vw] max-w-[80vw] sm:max-w-[80vw] max-h-[90vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>Configure Project Hints (.biorouterhints)</DialogTitle>
          <DialogDescription>
            Provide additional context about your project to improve communication with BioRouter
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 pt-2 pb-4">
          <HelpText />

          <div>
            {biorouterHintsFileReadError ? (
              <ErrorDisplay error={new Error(biorouterHintsFileReadError)} />
            ) : (
              <div className="space-y-2">
                <FileInfo filePath={biorouterHintsFilePath} found={biorouterHintsFileFound} />
                <textarea
                  value={biorouterHintsFile}
                  className="w-full h-80 border rounded-md p-2 text-sm resize-none bg-background-default text-textStandard border-borderStandard focus:outline-none focus:ring-2 focus:ring-blue-500"
                  onChange={(event) => setBioRouterHintsFile(event.target.value)}
                  placeholder="Enter project hints here..."
                />
              </div>
            )}
          </div>
        </div>

        <DialogFooter>
          {saveSuccess && (
            <span className="text-green-600 text-sm flex items-center gap-1 mr-auto">
              <Check className="w-4 h-4" />
              Saved successfully
            </span>
          )}
          <Button variant="outline" onClick={() => setIsBioRouterhintsModalOpen(false)}>
            Close
          </Button>
          <Button onClick={writeFile} disabled={isSaving}>
            {isSaving ? 'Saving...' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
