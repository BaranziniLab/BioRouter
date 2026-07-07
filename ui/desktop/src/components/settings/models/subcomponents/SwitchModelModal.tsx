import { useEffect, useState, useCallback, useMemo } from 'react';
import { Brain, ExternalLink } from '../../../icons/app-icons';

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../../ui/dialog';
import { Button } from '../../../ui/button';
import { QUICKSTART_GUIDE_URL } from '../../providers/modal/constants';
import { Input } from '../../../ui/input';
import { Select } from '../../../ui/Select';
import { useConfig } from '../../../ConfigContext';
import { useModelAndProvider } from '../../../ModelAndProviderContext';
import type { View } from '../../../../utils/navigationUtils';
import Model, { getProviderMetadata, fetchModelsForProviders } from '../modelInterface';
import { getPredefinedModelsFromEnv, shouldShowPredefinedModels } from '../predefinedModelsUtils';
import {
  llamacppStatus,
  ProviderType,
  type LlamaCppModel,
  type ProviderDetails,
} from '../../../../api';

// Return the first concrete model from the provider's list. The list is
// authored in priority order in the Rust provider definition (typically newest
// / preferred model first), so picking [0] gives the right default per provider
// without falling back to cross-provider name heuristics.
function findFirstAvailableModel(
  models: { value: string; label: string; provider: string }[]
): string | null {
  const validModels = models.filter(
    (m) => m.value !== 'custom' && m.value !== '__loading__' && !m.value.startsWith('__')
  );
  if (validModels.length === 0) return null;
  return validModels[0].value;
}

const llamaDownloadLabel = (model: LlamaCppModel | undefined) => {
  switch (model?.download_status) {
    case 'downloaded':
      return 'Downloaded';
    case 'partial':
      return 'Partial download';
    case 'not_downloaded':
      return 'Needs download';
    default:
      return null;
  }
};

const llamaModelLabel = (modelName: string, catalog: Map<string, LlamaCppModel>) => {
  const entry = catalog.get(modelName);
  if (!entry) return modelName;

  return [
    entry.display_name,
    entry.download_size,
    llamaDownloadLabel(entry),
    entry.description.toLowerCase().includes('community') ? 'Community GGUF' : null,
  ]
    .filter(Boolean)
    .join(' · ');
};

type SwitchModelModalProps = {
  sessionId: string | null;
  onClose: () => void;
  setView: (view: View) => void;
  onModelSelected?: (model: string) => void;
  initialProvider?: string | null;
  titleOverride?: string;
};
export const SwitchModelModal = ({
  sessionId,
  onClose,
  setView,
  onModelSelected,
  initialProvider,
  titleOverride,
}: SwitchModelModalProps) => {
  const { getProviders, getProviderModels, read } = useConfig();
  const { changeModel, currentModel, currentProvider } = useModelAndProvider();
  const [providerOptions, setProviderOptions] = useState<{ value: string; label: string }[]>([]);
  const [activeProviders, setActiveProviders] = useState<ProviderDetails[]>([]);
  type ModelOption = {
    value: string;
    label: string;
    provider: string;
    providerType?: ProviderType;
    isDisabled?: boolean;
  };
  const [modelOptionsByProvider, setModelOptionsByProvider] = useState<
    Record<string, ModelOption[]>
  >({});
  const [provider, setProvider] = useState<string | null>(
    initialProvider || currentProvider || null
  );
  // Only carry over the currently-running model when the provider being
  // configured matches the current chat provider. When switching providers
  // (e.g. opening the dialog with initialProvider=openai while running
  // anthropic), start empty so the auto-select effect picks the first model
  // for the selected provider instead of showing a model from the wrong one.
  const initialProviderResolved = initialProvider || currentProvider || null;
  const carryOverCurrentModel =
    !!currentModel && !!currentProvider && currentProvider === initialProviderResolved;
  const [model, setModel] = useState<string>(carryOverCurrentModel ? currentModel : '');
  const [isCustomModel, setIsCustomModel] = useState(false);
  const [validationErrors, setValidationErrors] = useState({
    provider: '',
    model: '',
  });
  const [isValid, setIsValid] = useState(true);
  const [attemptedSubmit, setAttemptedSubmit] = useState(false);
  const [usePredefinedModels] = useState(shouldShowPredefinedModels());
  const [selectedPredefinedModel, setSelectedPredefinedModel] = useState<Model | null>(null);
  const [predefinedModels, setPredefinedModels] = useState<Model[]>([]);
  const [loadingModels, setLoadingModels] = useState<boolean>(false);
  const [userClearedModel, setUserClearedModel] = useState(false);
  const [modelInputValue, setModelInputValue] = useState('');

  // Validate form data
  const validateForm = useCallback(() => {
    const errors = {
      provider: '',
      model: '',
    };
    let formIsValid = true;

    if (usePredefinedModels) {
      if (!selectedPredefinedModel) {
        errors.model = 'Please select a model';
        formIsValid = false;
      }
    } else {
      if (!provider) {
        errors.provider = 'Please select a provider';
        formIsValid = false;
      }

      if (!model) {
        errors.model = 'Please select or enter a model';
        formIsValid = false;
      }
    }

    setValidationErrors(errors);
    setIsValid(formIsValid);
    return formIsValid;
  }, [model, provider, usePredefinedModels, selectedPredefinedModel]);

  const handleClose = () => {
    onClose();
  };

  const handleSubmit = async () => {
    setAttemptedSubmit(true);
    const isFormValid = validateForm();

    if (isFormValid) {
      let modelObj: Model;

      if (usePredefinedModels && selectedPredefinedModel) {
        modelObj = selectedPredefinedModel;
      } else {
        const providerMetaData = await getProviderMetadata(provider || '', getProviders);
        const providerDisplayName = providerMetaData.display_name;
        modelObj = { name: model, provider: provider, subtext: providerDisplayName } as Model;
      }

      const changed = await changeModel(sessionId, modelObj);
      if (!changed) {
        return;
      }

      if (onModelSelected) {
        onModelSelected(modelObj.name);
      }
      onClose();
    }
  };

  // Re-validate when inputs change and after attempted submission
  useEffect(() => {
    if (attemptedSubmit) {
      validateForm();
    }
  }, [attemptedSubmit, validateForm]);

  useEffect(() => {
    // Load predefined models if enabled
    if (usePredefinedModels) {
      const models = getPredefinedModelsFromEnv();
      setPredefinedModels(models);

      // Initialize selected predefined model with current model
      (async () => {
        try {
          const currentModelName = (await read('BIOROUTER_MODEL', false)) as string;
          const matchingModel = models.find((model) => model.name === currentModelName);
          if (matchingModel) {
            setSelectedPredefinedModel(matchingModel);
          }
        } catch (error) {
          console.error('Failed to get current model for selection:', error);
        }
      })();
    }

    // Load providers for manual model selection.
    //
    // getProviders(false) reuses the cached list when ConfigContext already
    // has it; getProviders(true) forces a refetch and then calls
    // setProvidersList, which mutates the ConfigContext state that the
    // useCallback closes over — that bumps the getProviders reference, which
    // re-fires THIS effect (getProviders is a dep), which calls
    // getProviders(true) again. The result is a render loop that re-fires
    // setModelOptions / setLoadingModels several times per second and makes
    // react-select flicker its ClearIndicator (the X) as it churns. The
    // cached list is fine here — the user can reopen the modal to pick up
    // newly-configured providers.
    (async () => {
      try {
        const providersResponse = await getProviders(false);
        const activeProviders = providersResponse.filter((provider) => provider.is_configured);
        setActiveProviders(activeProviders);
        // Create provider options and add "Use other provider" option
        setProviderOptions([
          ...activeProviders.map(({ metadata, name }) => ({
            value: name,
            label: metadata.display_name,
          })),
          {
            value: 'configure_providers',
            label: 'Use other provider',
          },
        ]);
      } catch (error: unknown) {
        console.error('Failed to query providers:', error);
      }
    })();
  }, [getProviders, usePredefinedModels, read]);

  useEffect(() => {
    if (usePredefinedModels || !provider || modelOptionsByProvider[provider]) return;

    const selectedProvider = activeProviders.find((p) => p.name === provider);
    if (!selectedProvider) return;

    let cancelled = false;
    setLoadingModels(true);

    (async () => {
      try {
        const [result] = await fetchModelsForProviders([selectedProvider], getProviderModels);
        if (cancelled || !result) return;

        const modelList = result.error
          ? selectedProvider.metadata.known_models?.map(({ name }) => name) || []
          : result.models || [];

        if (result.error) {
          console.error('Provider model fetch errors:', [result.error]);
        }

        let llamaCatalog = new Map<string, LlamaCppModel>();
        if (selectedProvider.name === 'llamacpp') {
          try {
            const status = await llamacppStatus({ throwOnError: true });
            llamaCatalog = new Map(
              (status.data?.catalog || []).map((entry) => [entry.name, entry])
            );
          } catch (error) {
            console.error('Failed to query Llama Server catalog:', error);
          }
        }

        const options: ModelOption[] = modelList.map((m) => ({
          value: m,
          label: llamaModelLabel(m, llamaCatalog),
          provider: selectedProvider.name,
          providerType: selectedProvider.provider_type,
        }));

        if (selectedProvider.metadata.allows_unlisted_models) {
          options.push({
            value: 'custom',
            label: 'Enter a model not listed...',
            provider: selectedProvider.name,
            providerType: selectedProvider.provider_type,
          });
        }

        setModelOptionsByProvider((current) => ({
          ...current,
          [selectedProvider.name]: options,
        }));
      } catch (error) {
        console.error('Failed to query provider models:', error);
      } finally {
        if (!cancelled) setLoadingModels(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [activeProviders, getProviderModels, modelOptionsByProvider, provider, usePredefinedModels]);

  // Memoize so passing the same selection through to react-select doesn't
  // create a new array reference on every render — fresh references make
  // react-select treat its options list as "changed" and re-render the
  // ClearIndicator (the X button), which the user sees as a flicker while
  // interacting with the model dropdown.
  const filteredModelOptions = useMemo(() => {
    if (!provider) return [];

    const providerOptions = modelOptionsByProvider[provider] ?? [];
    const providerGroups = providerOptions.length > 0 ? [{ options: providerOptions }] : [];
    const trimmedInput = modelInputValue.trim();
    if (!trimmedInput) return providerGroups;

    const loweredInput = trimmedInput.toLowerCase();
    const matchingOptions = providerGroups
      .map((group) => ({
        options: group.options.filter(
          (option) => option.value.toLowerCase().includes(loweredInput) && option.value !== 'custom'
        ),
      }))
      .filter((group) => group.options.length > 0);

    if (matchingOptions.length > 0) return matchingOptions;

    const allowsCustomModel = providerGroups.some((group) =>
      group.options.some((option) => option.value === 'custom')
    );
    if (!allowsCustomModel) return [];

    return [
      {
        options: [
          {
            value: trimmedInput,
            label: `Use: "${trimmedInput}"`,
            provider,
          },
        ],
      },
    ];
  }, [provider, modelOptionsByProvider, modelInputValue]);

  // Same reason — a stable value object keeps the ClearIndicator stable.
  const modelSelectValue = useMemo(() => {
    if (!model) return null;

    const selectedOption = provider
      ? modelOptionsByProvider[provider]?.find((option) => option.value === model)
      : null;
    return { value: model, label: selectedOption?.label || model };
  }, [model, modelOptionsByProvider, provider]);
  const providerSelectValue = useMemo(
    () => providerOptions.find((option) => option.value === provider) || null,
    [providerOptions, provider]
  );

  useEffect(() => {
    // Don't auto-select if user explicitly cleared the model
    if (!provider || loadingModels || model || isCustomModel || userClearedModel) return;

    const providerModels = modelOptionsByProvider[provider] ?? [];

    if (providerModels.length > 0) {
      const firstModel = findFirstAvailableModel(providerModels);
      if (firstModel) {
        setModel(firstModel);
      }
    }
  }, [provider, modelOptionsByProvider, loadingModels, model, isCustomModel, userClearedModel]);

  // Handle model selection change
  const handleModelChange = (newValue: unknown) => {
    const selectedOption = newValue as { value: string; label: string; provider: string } | null;
    if (selectedOption?.value === 'custom') {
      setIsCustomModel(true);
      setModel('');
      setProvider(selectedOption.provider);
      setUserClearedModel(false);
      setModelInputValue('');
    } else if (selectedOption === null) {
      // User cleared the selection
      setIsCustomModel(false);
      setModel('');
      setUserClearedModel(true);
      setModelInputValue('');
    } else {
      setIsCustomModel(false);
      setModel(selectedOption?.value || '');
      setProvider(selectedOption?.provider || '');
      setUserClearedModel(false);
      setModelInputValue('');
    }
  };

  const handleInputChange = (inputValue: string, actionMeta?: { action?: string }): string => {
    if (!provider) return inputValue;

    if (!actionMeta || actionMeta.action === 'input-change') {
      setModelInputValue(inputValue);
    } else if (actionMeta.action === 'menu-close') {
      setModelInputValue('');
    }

    return inputValue;
  };

  return (
    <Dialog open={true} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Brain size={24} className="text-text-default" />
            {titleOverride || 'Switch models'}
          </DialogTitle>
          <DialogDescription>
            Select a provider and model to use for your conversations.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4 py-4">
          {usePredefinedModels ? (
            <div className="w-full flex flex-col gap-4">
              <div className="flex justify-between items-center">
                <label className="text-sm font-medium text-text-default">Choose a model:</label>
              </div>

              <div className="flex flex-col max-h-72 overflow-y-auto -mx-1 px-1">
                {predefinedModels.map((model) => {
                  const isSelected = selectedPredefinedModel?.name === model.name;
                  return (
                    <div
                      key={model.id || model.name}
                      onClick={() => setSelectedPredefinedModel(model)}
                      className={[
                        'biorouter-modal-row flex items-start gap-3 py-2.5 px-3 rounded-xl cursor-pointer transition-colors duration-150',
                        isSelected
                          ? '!border-border-default bg-background-medium'
                          : 'hover:!border-border-default hover:bg-background-medium',
                      ].join(' ')}
                    >
                      {/* Radio dot */}
                      <div
                        className={[
                          'mt-0.5 h-4 w-4 rounded-full border-2 flex-shrink-0 flex items-center justify-center transition-all duration-150',
                          isSelected
                            ? 'border-text-default bg-text-default'
                            : 'border-border-subtle',
                        ].join(' ')}
                      >
                        {isSelected && (
                          <div className="h-1.5 w-1.5 rounded-full bg-background-default" />
                        )}
                      </div>

                      {/* Model info */}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="text-sm font-medium text-text-default">
                            {model.alias || model.name}
                          </span>
                          {model.alias?.toLowerCase().includes('recommended') && (
                            <span className="text-[11px] font-medium uppercase tracking-wider text-text-muted bg-background-default/80 px-1.5 py-0.5 rounded-md shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--border-subtle)_55%,transparent)]">
                              Recommended
                            </span>
                          )}
                        </div>
                        {model.subtext && (
                          <p className="text-xs text-text-muted mt-0.5 line-clamp-2 leading-relaxed">
                            {model.subtext}
                          </p>
                        )}
                      </div>

                      {/* Provider badge */}
                      {model.provider && (
                        <span className="text-[11px] text-text-muted bg-background-default/80 px-1.5 py-0.5 rounded-md flex-shrink-0 mt-0.5 shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--border-subtle)_55%,transparent)]">
                          {model.provider}
                        </span>
                      )}
                    </div>
                  );
                })}
              </div>

              {attemptedSubmit && validationErrors.model && (
                <div className="text-text-danger text-sm mt-1">{validationErrors.model}</div>
              )}
            </div>
          ) : (
            /* Manual Provider/Model Selection */
            <div className="w-full flex flex-col gap-4">
              <div>
                <Select
                  options={providerOptions}
                  value={providerSelectValue}
                  onChange={(newValue: unknown) => {
                    const option = newValue as { value: string; label: string } | null;
                    if (option?.value === 'configure_providers') {
                      // Navigate to ConfigureProviders view
                      setView('ConfigureProviders');
                      onClose(); // Close the current modal
                    } else {
                      setProvider(option?.value || null);
                      setModel('');
                      setIsCustomModel(false);
                      setUserClearedModel(false);
                      setModelInputValue('');
                    }
                  }}
                  placeholder="Provider, type to search"
                  isClearable
                />
                {attemptedSubmit && validationErrors.provider && (
                  <div className="text-text-danger text-sm mt-1">{validationErrors.provider}</div>
                )}
              </div>

              {provider && (
                <>
                  {!isCustomModel ? (
                    <div>
                      <Select
                        options={loadingModels ? [] : filteredModelOptions}
                        onChange={handleModelChange}
                        onInputChange={handleInputChange}
                        inputValue={modelInputValue}
                        value={modelSelectValue}
                        placeholder={
                          loadingModels ? 'Loading models…' : 'Select a model, type to search'
                        }
                        isClearable
                        isDisabled={loadingModels}
                      />

                      {attemptedSubmit && validationErrors.model && (
                        <div className="text-text-danger text-sm mt-1">
                          {validationErrors.model}
                        </div>
                      )}
                    </div>
                  ) : (
                    <div className="flex flex-col gap-2">
                      <div className="flex justify-between">
                        <label className="text-sm text-text-muted">Custom model name</label>
                        <button
                          onClick={() => setIsCustomModel(false)}
                          className="text-sm text-text-muted"
                        >
                          Back to model list
                        </button>
                      </div>
                      <Input
                        className="border-2 px-4 py-5"
                        placeholder="Type model name here"
                        onChange={(event) => setModel(event.target.value)}
                        value={model}
                      />
                      {attemptedSubmit && validationErrors.model && (
                        <div className="text-text-danger text-sm mt-1">
                          {validationErrors.model}
                        </div>
                      )}
                    </div>
                  )}
                </>
              )}
            </div>
          )}
        </div>

        <DialogFooter className="pt-4 flex-col sm:flex-row gap-3">
          <a
            href={QUICKSTART_GUIDE_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center text-text-muted hover:text-text-default text-sm mr-auto"
          >
            <ExternalLink size={14} className="mr-1" />
            Quick start guide
          </a>
          <div className="flex gap-2">
            <Button variant="outline" onClick={handleClose} type="button">
              Cancel
            </Button>
            <Button onClick={handleSubmit} disabled={!isValid}>
              Select model
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
