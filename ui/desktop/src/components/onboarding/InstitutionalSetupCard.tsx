import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useConfig } from '../ConfigContext';
import { checkProvider } from '../../api';
import { Button } from '../ui/button';
import { ArrowRight } from '../icons/ArrowRight';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '../ui/collapsible';
import OnboardingSectionLabel from './OnboardingSectionLabel';

interface InstitutionalSetupCardProps {
  onSuccess: (provider: string) => void;
  onStartTesting?: () => void;
}

type VersaFlavor = 'azure' | 'bedrock';

const VERSA_BEDROCK_DEFAULTS = {
  AWS_ENDPOINT_URL_BEDROCK: 'https://unified-api.ucsf.edu/general/awsai',
  AWS_REGION: 'us-west-2',
};

const VERSA_AZURE_DEFAULTS = {
  VERSA_AZURE_ENDPOINT: 'https://unified-api.ucsf.edu/general',
  VERSA_AZURE_DEPLOYMENT_NAME: 'gpt-5.5-2026-04-24',
  VERSA_AZURE_API_VERSION: '2025-01-01-preview',
};

const TABS: { id: VersaFlavor; label: string }[] = [
  { id: 'azure', label: 'Azure OpenAI (GPT)' },
  { id: 'bedrock', label: 'Bedrock (Claude)' },
];

const inputClass =
  'w-full h-9 px-3 text-sm border border-border-subtle rounded-md bg-background-default text-text-default placeholder:text-text-muted  focus:border-border-strong transition-colors duration-150';

const advancedInputClass =
  'w-full h-8 px-3 text-xs border border-border-subtle rounded-md bg-background-default text-text-default  focus:border-border-strong';

export default function InstitutionalSetupCard({
  onSuccess,
  onStartTesting,
}: InstitutionalSetupCardProps) {
  const { upsert } = useConfig();
  const navigate = useNavigate();
  const [flavor, setFlavor] = useState<VersaFlavor>('azure');
  const [bedrockAccessKey, setBedrockAccessKey] = useState('');
  const [bedrockSecretKey, setBedrockSecretKey] = useState('');
  const [azureApiKey, setAzureApiKey] = useState('');
  const [bedrockEndpoint, setBedrockEndpoint] = useState(
    VERSA_BEDROCK_DEFAULTS.AWS_ENDPOINT_URL_BEDROCK
  );
  const [bedrockRegion, setBedrockRegion] = useState(VERSA_BEDROCK_DEFAULTS.AWS_REGION);
  const [azureEndpoint, setAzureEndpoint] = useState(VERSA_AZURE_DEFAULTS.VERSA_AZURE_ENDPOINT);
  const [azureDeployment, setAzureDeployment] = useState(
    VERSA_AZURE_DEFAULTS.VERSA_AZURE_DEPLOYMENT_NAME
  );
  const [azureApiVersion, setAzureApiVersion] = useState(
    VERSA_AZURE_DEFAULTS.VERSA_AZURE_API_VERSION
  );
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  const canSubmit =
    !isLoading &&
    (flavor === 'bedrock'
      ? bedrockAccessKey.trim().length > 0 && bedrockSecretKey.trim().length > 0
      : azureApiKey.trim().length > 0);

  const handleSubmit = async () => {
    if (!canSubmit) return;
    onStartTesting?.();
    setIsLoading(true);
    setError(null);

    try {
      if (flavor === 'bedrock') {
        await upsert('VERSA_BEDROCK_ACCESS_KEY_ID', bedrockAccessKey.trim(), true);
        await upsert('VERSA_BEDROCK_SECRET_ACCESS_KEY', bedrockSecretKey.trim(), true);
        await upsert('AWS_ENDPOINT_URL_BEDROCK', bedrockEndpoint.trim(), false);
        await upsert('AWS_REGION', bedrockRegion.trim(), false);
        await checkProvider({ body: { provider: 'versa_bedrock' }, throwOnError: true });
        await upsert('BIOROUTER_PROVIDER', 'versa_bedrock', false);
        onSuccess('versa_bedrock');
      } else {
        await upsert('VERSA_AZURE_API_KEY', azureApiKey.trim(), true);
        await upsert('VERSA_AZURE_ENDPOINT', azureEndpoint.trim(), false);
        await upsert('VERSA_AZURE_DEPLOYMENT_NAME', azureDeployment.trim(), false);
        await upsert('VERSA_AZURE_API_VERSION', azureApiVersion.trim(), false);
        await checkProvider({ body: { provider: 'versa_azure' }, throwOnError: true });
        await upsert('BIOROUTER_PROVIDER', 'versa_azure', false);
        onSuccess('versa_azure');
      }
    } catch (e) {
      setError(
        e instanceof Error
          ? `Could not connect: ${e.message}`
          : 'Could not connect with these credentials.'
      );
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <section
      aria-labelledby="institutional-setup-title"
      className="min-w-0 overflow-hidden rounded-xl border border-border-subtle bg-background-card p-5 sm:p-6"
    >
      <OnboardingSectionLabel category="institutional" label="Institutional · UCSF Versa API" />
      <h2 id="institutional-setup-title" className="mt-2 text-base font-medium text-text-default">
        UCSF-hosted models
      </h2>
      <p className="text-sm text-text-muted mt-1 mb-5 leading-relaxed">
        Use UCSF-hosted models through the Versa unified API. Best for UCSF affiliates.
      </p>

      {/* Tab toggle */}
      <div
        role="tablist"
        aria-label="UCSF-hosted provider"
        className="mb-4 grid w-full grid-cols-2 rounded-md bg-background-medium p-0.5 sm:inline-grid sm:w-auto"
      >
        {TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => {
              setFlavor(tab.id);
              setError(null);
            }}
            role="tab"
            aria-selected={flavor === tab.id}
            className={`h-7 min-w-0 truncate rounded px-2 text-xs transition-colors duration-150 sm:px-3 ${flavor === tab.id ? 'bg-background-default text-text-default' : 'text-text-muted hover:text-text-default'}`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {flavor === 'azure' ? (
        <div>
          <label
            htmlFor="versa-azure-api-key"
            className="mb-1.5 block text-xs font-medium text-text-default"
          >
            API Key
          </label>
          <input
            id="versa-azure-api-key"
            type="password"
            value={azureApiKey}
            onChange={(e) => setAzureApiKey(e.target.value)}
            placeholder="VERSA_AZURE_API_KEY"
            className={inputClass}
            disabled={isLoading}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && canSubmit) handleSubmit();
            }}
          />
        </div>
      ) : (
        <div className="space-y-3">
          <div>
            <label
              htmlFor="versa-bedrock-access-key"
              className="mb-1.5 block text-xs font-medium text-text-default"
            >
              Access Key ID
            </label>
            <input
              id="versa-bedrock-access-key"
              type="password"
              value={bedrockAccessKey}
              onChange={(e) => setBedrockAccessKey(e.target.value)}
              placeholder="VERSA_BEDROCK_ACCESS_KEY_ID"
              className={inputClass}
              disabled={isLoading}
            />
          </div>
          <div>
            <label
              htmlFor="versa-bedrock-secret-key"
              className="mb-1.5 block text-xs font-medium text-text-default"
            >
              Secret Access Key
            </label>
            <input
              id="versa-bedrock-secret-key"
              type="password"
              value={bedrockSecretKey}
              onChange={(e) => setBedrockSecretKey(e.target.value)}
              placeholder="VERSA_BEDROCK_SECRET_ACCESS_KEY"
              className={inputClass}
              disabled={isLoading}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && canSubmit) handleSubmit();
              }}
            />
          </div>
        </div>
      )}

      <Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen} className="mt-3">
        <CollapsibleTrigger className="flex items-center gap-1.5 text-xs text-text-muted hover:text-text-default transition-colors duration-150">
          <span>{advancedOpen ? '▾' : '▸'}</span>
          <span>
            Advanced (
            {flavor === 'azure' ? 'endpoint, deployment, API version' : 'endpoint, region'})
          </span>
        </CollapsibleTrigger>
        <CollapsibleContent className="mt-2.5 pl-3.5 space-y-2.5 border-l border-border-default">
          {flavor === 'azure' ? (
            <>
              <div>
                <label className="block text-[11px] text-text-muted mb-1">
                  VERSA_AZURE_ENDPOINT
                </label>
                <input
                  type="text"
                  value={azureEndpoint}
                  onChange={(e) => setAzureEndpoint(e.target.value)}
                  className={advancedInputClass}
                  disabled={isLoading}
                />
              </div>
              <div>
                <label className="block text-[11px] text-text-muted mb-1">
                  VERSA_AZURE_DEPLOYMENT_NAME
                </label>
                <input
                  type="text"
                  value={azureDeployment}
                  onChange={(e) => setAzureDeployment(e.target.value)}
                  className={advancedInputClass}
                  disabled={isLoading}
                />
              </div>
              <div>
                <label className="block text-[11px] text-text-muted mb-1">
                  VERSA_AZURE_API_VERSION
                </label>
                <input
                  type="text"
                  value={azureApiVersion}
                  onChange={(e) => setAzureApiVersion(e.target.value)}
                  className={advancedInputClass}
                  disabled={isLoading}
                />
              </div>
            </>
          ) : (
            <>
              <div>
                <label className="block text-[11px] text-text-muted mb-1">
                  AWS_ENDPOINT_URL_BEDROCK
                </label>
                <input
                  type="text"
                  value={bedrockEndpoint}
                  onChange={(e) => setBedrockEndpoint(e.target.value)}
                  className={advancedInputClass}
                  disabled={isLoading}
                />
              </div>
              <div>
                <label className="block text-[11px] text-text-muted mb-1">AWS_REGION</label>
                <input
                  type="text"
                  value={bedrockRegion}
                  onChange={(e) => setBedrockRegion(e.target.value)}
                  className={advancedInputClass}
                  disabled={isLoading}
                />
              </div>
            </>
          )}
        </CollapsibleContent>
      </Collapsible>

      <div className="mt-5 flex flex-col items-stretch gap-2 sm:flex-row sm:flex-wrap sm:items-center sm:gap-x-4 sm:gap-y-3">
        <Button onClick={handleSubmit} disabled={!canSubmit} className="h-9 w-full px-4 sm:w-auto">
          {isLoading ? (
            <div className="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" />
          ) : (
            <>
              Connect to {flavor === 'azure' ? 'Versa Azure OpenAI' : 'Versa Bedrock'}
              <ArrowRight className="w-4 h-4" />
            </>
          )}
        </Button>
        <button
          type="button"
          onClick={() => navigate('/welcome', { replace: true })}
          className="w-full py-1 text-center text-xs text-text-muted transition-colors duration-150 hover:text-text-default sm:w-auto sm:text-left"
        >
          View all institutional providers →
        </button>
      </div>

      {error && (
        <div className="mt-3 text-xs p-2.5 rounded-md bg-background-danger/10 text-text-danger border border-border-danger/40">
          {error}
        </div>
      )}
    </section>
  );
}
