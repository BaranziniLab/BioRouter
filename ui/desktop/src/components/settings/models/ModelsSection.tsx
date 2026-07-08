import { useEffect, useState, useCallback, useRef } from 'react';
import { View } from '../../../utils/navigationUtils';
import ModelSettingsButtons from './subcomponents/ModelSettingsButtons';
import { useConfig } from '../../ConfigContext';
import {
  UNKNOWN_PROVIDER_MSG,
  UNKNOWN_PROVIDER_TITLE,
  useModelAndProvider,
} from '../../ModelAndProviderContext';
import { toastError } from '../../../toasts';
import ResetProviderSection from '../reset_provider/ResetProviderSection';
import LocalModelInventory from './LocalModelInventory';

interface ModelsSectionProps {
  setView: (view: View) => void;
}

export default function ModelsSection({ setView }: ModelsSectionProps) {
  const [provider, setProvider] = useState<string | null>(null);
  const [displayModelName, setDisplayModelName] = useState<string>('');
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const { read, getProviders } = useConfig();
  const {
    getCurrentModelDisplayName,
    getCurrentProviderDisplayName,
    currentModel,
    currentProvider,
  } = useModelAndProvider();

  const loadModelData = useCallback(async () => {
    try {
      setIsLoading(true);

      const modelDisplayName = await getCurrentModelDisplayName();
      setDisplayModelName(modelDisplayName);

      const providerDisplayName = await getCurrentProviderDisplayName();
      if (providerDisplayName) {
        setProvider(providerDisplayName);
      } else {
        const providerName = (await read('BIOROUTER_PROVIDER', false)) as string;
        const providers = await getProviders(false);
        const providerDetailsList = providers.filter((provider) => provider.name === providerName);

        if (providerDetailsList.length != 1) {
          toastError({
            title: UNKNOWN_PROVIDER_TITLE,
            msg: UNKNOWN_PROVIDER_MSG,
          });
          setProvider(providerName);
        } else {
          const fallbackProviderDisplayName = providerDetailsList[0].metadata.display_name;
          setProvider(fallbackProviderDisplayName);
        }
      }
    } catch (error) {
      console.error('Error loading model data:', error);
    } finally {
      setIsLoading(false);
    }
  }, [read, getProviders, getCurrentModelDisplayName, getCurrentProviderDisplayName]);

  useEffect(() => {
    loadModelData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const prevModelRef = useRef<string | null>(null);
  const prevProviderRef = useRef<string | null>(null);

  useEffect(() => {
    if (
      currentModel &&
      currentProvider &&
      (currentModel !== prevModelRef.current || currentProvider !== prevProviderRef.current)
    ) {
      prevModelRef.current = currentModel;
      prevProviderRef.current = currentProvider;
      loadModelData();
    }
  }, [currentModel, currentProvider, loadModelData]);

  return (
    <section id="models" className="pb-8">
      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
            Current Model
          </h2>
        </div>
        <div className="biorouter-settings-list">
          <div className="biorouter-settings-row px-3 py-3">
            {isLoading ? (
              <>
                <div className="h-5 mb-1.5 bg-background-medium rounded w-48 animate-pulse"></div>
                <div className="h-4 bg-background-medium rounded w-32 animate-pulse"></div>
              </>
            ) : (
              <div className="animate-in fade-in duration-100">
                <p className="text-sm font-medium text-text-default">{displayModelName}</p>
                <p className="text-xs text-text-muted mt-0.5">{provider}</p>
              </div>
            )}
            <ModelSettingsButtons setView={setView} />
          </div>
        </div>
      </div>

      <LocalModelInventory />

      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
            Reset
          </h2>
        </div>
        <div className="biorouter-settings-list">
          <ResetProviderSection setView={setView} />
        </div>
      </div>
    </section>
  );
}
