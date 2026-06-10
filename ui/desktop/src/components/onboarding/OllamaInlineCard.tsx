import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useConfig } from '../ConfigContext';
import { toastService } from '../../toasts';
import { Button } from '../ui/button';
import {
  checkOllamaStatus,
  getOllamaDownloadUrl,
  pollForOllama,
  hasModel,
  pullOllamaModel,
  getPreferredModel,
  type PullProgress,
} from '../../utils/ollamaDetection';
import OnboardingSectionLabel from './OnboardingSectionLabel';

interface OllamaInlineCardProps {
  onSuccess: () => void;
}

type ModelStatus = 'checking' | 'available' | 'not-available' | 'downloading';

export default function OllamaInlineCard({ onSuccess }: OllamaInlineCardProps) {
  const navigate = useNavigate();
  const { upsert } = useConfig();
  const [isChecking, setIsChecking] = useState(true);
  const [ollamaDetected, setOllamaDetected] = useState(false);
  const [isPolling, setIsPolling] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [modelStatus, setModelStatus] = useState<ModelStatus>('checking');
  const [downloadProgress, setDownloadProgress] = useState<PullProgress | null>(null);
  const stopPollingRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const checkInitial = async () => {
      const status = await checkOllamaStatus();
      setOllamaDetected(status.isRunning);
      if (status.isRunning) {
        const modelAvailable = await hasModel(getPreferredModel());
        setModelStatus(modelAvailable ? 'available' : 'not-available');
      }
      setIsChecking(false);
    };
    checkInitial();
    return () => {
      if (stopPollingRef.current) stopPollingRef.current();
    };
  }, []);

  const handleInstallClick = () => {
    setIsPolling(true);
    stopPollingRef.current = pollForOllama(async (status) => {
      setOllamaDetected(status.isRunning);
      setIsPolling(false);
      const modelAvailable = await hasModel(getPreferredModel());
      setModelStatus(modelAvailable ? 'available' : 'not-available');
      toastService.success({
        title: 'Ollama Detected!',
        msg: 'Ollama is now running. You can connect to it.',
      });
    }, 3000);
  };

  const handleDownloadModel = async () => {
    setModelStatus('downloading');
    setDownloadProgress({ status: 'Starting download...' });
    const success = await pullOllamaModel(getPreferredModel(), (progress) => {
      setDownloadProgress(progress);
    });
    if (success) {
      setModelStatus('available');
      toastService.success({
        title: 'Model Downloaded!',
        msg: `Successfully downloaded ${getPreferredModel()}`,
      });
    } else {
      setModelStatus('not-available');
      toastService.error({
        title: 'Download Failed',
        msg: `Failed to download ${getPreferredModel()}. Please try again.`,
        traceback: '',
      });
    }
    setDownloadProgress(null);
  };

  const handleConnectOllama = async () => {
    setIsConnecting(true);
    try {
      await upsert('BIOROUTER_PROVIDER', 'ollama', false);
      await upsert('BIOROUTER_MODEL', getPreferredModel(), false);
      await upsert('OLLAMA_HOST', 'localhost', false);
      toastService.success({
        title: 'Success!',
        msg: `Connected to Ollama with ${getPreferredModel()} model.`,
      });
      onSuccess();
    } catch (error) {
      console.error('Failed to connect to Ollama:', error);
      toastService.error({
        title: 'Connection Failed',
        msg: `Failed to connect to Ollama: ${error instanceof Error ? error.message : String(error)}`,
        traceback: error instanceof Error ? error.stack || '' : '',
      });
      setIsConnecting(false);
    }
  };

  const detectedPill = (
    <span className="inline-flex items-center gap-1.5 text-[11px] text-text-muted">
      <span className="w-1.5 h-1.5 rounded-full bg-background-success" />
      Ollama running · {getPreferredModel()}
      {modelStatus === 'available'
        ? ' ready'
        : modelStatus === 'not-available'
          ? ' not installed'
          : ''}
    </span>
  );

  const notDetectedPill = (
    <span className="inline-flex items-center gap-1.5 text-[11px] text-text-muted">
      <span className="w-1.5 h-1.5 rounded-full bg-background-warning" />
      Ollama not detected
    </span>
  );

  return (
    <section className="py-7 border-b border-border-default">
      <OnboardingSectionLabel category="local" label="Local · Run on your computer" />
      <h2 className="text-base font-medium text-text-default mt-2">Ollama</h2>
      <p className="text-sm text-text-muted mt-1 mb-5 leading-relaxed">
        Run open-source models on your own machine. Free, private, offline.
      </p>

      {isChecking ? (
        <div className="flex items-center gap-2 text-xs text-text-muted">
          <div className="w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin flex-shrink-0" />
          <span>Checking for Ollama…</span>
        </div>
      ) : ollamaDetected ? (
        <div className="space-y-3">
          <div>{detectedPill}</div>

          {modelStatus === 'checking' && (
            <div className="flex items-center gap-2 text-xs text-text-muted">
              <div className="w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin flex-shrink-0" />
              <span>Checking for model…</span>
            </div>
          )}

          {modelStatus === 'downloading' && downloadProgress && (
            <div className="rounded-md border border-border-default bg-background-default p-3">
              <p className="text-xs text-text-default">Downloading {getPreferredModel()}…</p>
              <p className="text-[11px] text-text-muted mt-1">{downloadProgress.status}</p>
              {downloadProgress.total && downloadProgress.completed && (
                <>
                  <div className="mt-2 bg-background-medium rounded-full h-1.5 overflow-hidden">
                    <div
                      className="bg-background-success h-full transition-all duration-300"
                      style={{
                        width: `${(downloadProgress.completed / downloadProgress.total) * 100}%`,
                      }}
                    />
                  </div>
                  <p className="text-[11px] text-text-muted mt-1">
                    {Math.round((downloadProgress.completed / downloadProgress.total) * 100)}%
                  </p>
                </>
              )}
            </div>
          )}

          <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
            {modelStatus === 'not-available' && (
              <Button onClick={handleDownloadModel} variant="outline" className="h-9 px-4">
                Download {getPreferredModel()} (~11GB)
              </Button>
            )}
            {modelStatus === 'available' && (
              <Button onClick={handleConnectOllama} disabled={isConnecting} className="h-9 px-4">
                {isConnecting ? 'Connecting…' : 'Connect to Ollama'}
              </Button>
            )}
            <button
              type="button"
              onClick={() => navigate('/welcome', { replace: true })}
              className="text-xs text-text-muted hover:text-text-default transition-colors duration-150"
            >
              View all local providers →
            </button>
          </div>
        </div>
      ) : (
        <div className="space-y-3">
          <div>{notDetectedPill}</div>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
            {isPolling ? (
              <div className="flex items-center gap-2 text-xs text-text-muted">
                <div className="w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin flex-shrink-0" />
                <span>Waiting for Ollama to start…</span>
              </div>
            ) : (
              <a
                href={getOllamaDownloadUrl()}
                target="_blank"
                rel="noopener noreferrer"
                onClick={handleInstallClick}
                className="inline-flex items-center justify-center h-9 px-4 bg-background-medium text-text-default hover:bg-background-strong transition-colors rounded-md text-sm"
              >
                Install Ollama
              </a>
            )}
            <button
              type="button"
              onClick={() => navigate('/welcome', { replace: true })}
              className="text-xs text-text-muted hover:text-text-default transition-colors duration-150"
            >
              View all local providers →
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
