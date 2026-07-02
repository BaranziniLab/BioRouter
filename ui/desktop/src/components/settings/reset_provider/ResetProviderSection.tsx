import { Button } from '../../ui/button';
import { RefreshCw } from '../../icons/app-icons';
import { useConfig } from '../../ConfigContext';
import { View, ViewOptions } from '../../../utils/navigationUtils';

interface ResetProviderSectionProps {
  setView: (view: View, viewOptions?: ViewOptions) => void;
}

export default function ResetProviderSection(_props: ResetProviderSectionProps) {
  const { remove } = useConfig();

  const handleResetProvider = async () => {
    try {
      await remove('BIOROUTER_PROVIDER', false);
      await remove('BIOROUTER_MODEL', false);

      // Refresh the page to trigger the ProviderGuard check
      window.location.reload();
    } catch (error) {
      console.error('Failed to reset provider and model:', error);
    }
  };

  return (
    <div className="biorouter-settings-row px-3 py-3">
      <p className="text-sm text-text-default mb-1">Reset Provider and Model</p>
      <p className="text-xs text-text-muted mb-4">
        This will clear your selected model and provider settings. If no defaults are available,
        you'll be taken to the welcome screen to set them up again.
      </p>
      <Button
        onClick={handleResetProvider}
        variant="destructive"
        className="flex items-center justify-center gap-2"
      >
        <RefreshCw className="h-4 w-4" />
        Reset Provider and Model
      </Button>
    </div>
  );
}
