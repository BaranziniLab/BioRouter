import { Switch } from '../../ui/switch';
import { useConfig } from '../../ConfigContext';

// The four backend config keys gating the BRSDK App-SDK safety frameworks.
// All default to false on the server — these toggles only opt a user in.
interface BrsdkConfig {
  brsdk_pii_guardrail?: boolean;
  brsdk_llm_guardrails?: boolean;
  brsdk_encryption?: boolean;
  brsdk_tracing?: boolean;
}

type BrsdkKey = keyof BrsdkConfig;

interface BrsdkToggleMeta {
  key: BrsdkKey;
  label: string;
  description: string;
}

const BRSDK_TOGGLES: BrsdkToggleMeta[] = [
  {
    key: 'brsdk_pii_guardrail',
    label: 'PII / PHI guardrail',
    description:
      'Mask personally identifiable and health information locally before it reaches an Agent-Drafter app. Opt-in, off by default.',
  },
  {
    key: 'brsdk_llm_guardrails',
    label: 'LLM guardrails',
    description:
      'Just-in-time LLM safety checks, including the goal Stop-hook judge, for Agent-Drafter apps. Opt-in, off by default.',
  },
  {
    key: 'brsdk_encryption',
    label: 'Encrypted vault',
    description:
      'Store each Agent-Drafter app’s data in a per-app encrypted vault. Opt-in, off by default.',
  },
  {
    key: 'brsdk_tracing',
    label: 'Agent tracing',
    description:
      'Record an agent trace timeline for Agent-Drafter apps to inspect what ran and when. Opt-in, off by default.',
  },
];

export const BrsdkSection = () => {
  const { config, upsert } = useConfig();
  const brsdkConfig = (config as BrsdkConfig) ?? {};

  const handleToggle = async (key: BrsdkKey, enabled: boolean) => {
    try {
      await upsert(key, enabled, false);
    } catch (error) {
      console.error(`Error updating ${key}:`, error);
    }
  };

  return (
    <div className="space-y-1">
      {BRSDK_TOGGLES.map((toggle) => {
        const enabled = brsdkConfig[toggle.key] ?? false;
        return (
          <div
            key={toggle.key}
            className="flex items-center justify-between py-3 px-2 -mx-2 hover:bg-background-muted rounded-lg transition-all"
          >
            <div>
              <h3 className="text-sm font-medium text-text-default">{toggle.label}</h3>
              <p className="text-xs text-text-muted max-w-md mt-[2px]">{toggle.description}</p>
            </div>
            <div className="flex items-center flex-shrink-0">
              <Switch
                checked={enabled}
                onCheckedChange={(checked) => handleToggle(toggle.key, checked)}
                variant="mono"
                aria-label={`Toggle ${toggle.label}`}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
};
