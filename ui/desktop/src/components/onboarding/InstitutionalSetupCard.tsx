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
  AZURE_OPENAI_ENDPOINT: 'https://unified-api.ucsf.edu/general',
  AZURE_OPENAI_DEPLOYMENT_NAME: 'gpt-5.2-2025-12-11',
  AZURE_OPENAI_API_VERSION: '2024-10-21',
};

const TABS: { id: VersaFlavor; label: string }[] = [
  { id: 'azure', label: 'Azure OpenAI (GPT)' },
  { id: 'bedrock', label: 'Bedrock (Claude)' },
];

const inputClass =
  'w-full h-9 px-3 text-sm border border-border-subtle rounded-md bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150';

const advancedInputClass =
  'w-full h-8 px-3 text-xs border border-border-subtle rounded-md bg-background-default text-text-default focus:outline-none focus:border-border-strong';

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
  const [azureEndpoint, setAzureEndpoint] = useState(VERSA_AZURE_DEFAULTS.AZURE_OPENAI_ENDPOINT);
  const [azureDeployment, setAzureDeployment] = useState(
    VERSA_AZURE_DEFAULTS.AZURE_OPENAI_DEPLOYMENT_NAME
  );
  const [azureApiVersion, setAzureApiVersion] = useState(
    VERSA_AZURE_DEFAULTS.AZURE_OPENAI_API_VERSION
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
        await upsert('AZURE_OPENAI_ENDPOINT', azureEndpoint.trim(), false);
        await upsert('AZURE_OPENAI_DEPLOYMENT_NAME', azureDeployment.trim(), false);
        await upsert('AZURE_OPENAI_API_VERSION', azureApiVersion.trim(), false);
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
    <section className="py-7 border-b border-border-default">
      <OnboardingSectionLabel category="institutional" label="Institutional · UCSF Versa API" />
      <h2 className="text-base font-medium text-text-default mt-2">UCSF-hosted models</h2>
      <p className="text-sm text-text-muted mt-1 mb-5 leading-relaxed">
        Use UCSF-hosted models through the Versa unified API. Best for UCSF affiliates.
      </p>

      {/* Tab toggle */}
      <div className="inline-flex rounded-md bg-background-medium p-0.5 mb-4">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => {
              setFlavor(tab.id);
              setError(null);
            }}
            className={`px-3 h-7 text-xs rounded transition-colors duration-150 ${
              flavor === tab.id
                ? 'bg-background-default text-text-default'
                : 'text-text-muted hover:text-text-default'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {flavor === 'azure' ? (
        <div>
          <label className="block text-xs font-medium text-text-default mb-1.5">API Key</label>
          <input
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
            <label className="block text-xs font-medium text-text-default mb-1.5">
              Access Key ID
            </label>
            <input
              type="password"
              value={bedrockAccessKey}
              onChange={(e) => setBedrockAccessKey(e.target.value)}
              placeholder="VERSA_BEDROCK_ACCESS_KEY_ID"
              className={inputClass}
              disabled={isLoading}
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-text-default mb-1.5">
              Secret Access Key
            </label>
            <input
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
                  AZURE_OPENAI_ENDPOINT
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
                  AZURE_OPENAI_DEPLOYMENT_NAME
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
                  AZURE_OPENAI_API_VERSION
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

      <div className="flex flex-wrap items-center gap-x-4 gap-y-3 mt-5">
        <Button onClick={handleSubmit} disabled={!canSubmit} className="h-9 px-4">
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
          className="text-xs text-text-muted hover:text-text-default transition-colors duration-150"
        >
          View all institutional providers →
        </button>
      </div>

      {error && (
        <div className="mt-3 text-xs p-2.5 rounded-md bg-red-50 dark:bg-red-900/20 text-red-800 dark:text-red-300 border border-red-200 dark:border-red-800/50">
          {error}
        </div>
      )}
    </section>
  );
}
