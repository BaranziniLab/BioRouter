import { useState, useEffect } from 'react';
import { Button } from '../../ui/button';
import { ExternalLink } from 'lucide-react';

const GITHUB_RELEASES_URL = 'https://github.com/BaranziniLab/BioRouter/releases';

export default function UpdateSection() {
  const [currentVersion, setCurrentVersion] = useState('');

  useEffect(() => {
    setCurrentVersion(window.electron.getVersion());
  }, []);

  return (
    <div>
      <div className="text-sm text-text-muted mb-4">
        <div className="flex flex-col">
          <div className="text-text-default text-2xl font-mono">
            {currentVersion || 'Loading...'}
          </div>
          <div className="text-xs text-text-muted">Current version</div>
        </div>
      </div>

      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-3">
          <Button
            onClick={() => window.open(GITHUB_RELEASES_URL, '_blank')}
            variant="secondary"
            size="sm"
            className="flex items-center gap-2"
          >
            <ExternalLink className="w-4 h-4" />
            Check for Updates
          </Button>
          <p className="text-xs text-text-muted">
            Opens GitHub Releases to check for and download the latest version.
          </p>
        </div>
      </div>
    </div>
  );
}
