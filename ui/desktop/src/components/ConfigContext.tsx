import React, {
  createContext,
  useContext,
  useState,
  useEffect,
  useMemo,
  useCallback,
  useRef,
} from 'react';
import {
  readAllConfig,
  readConfig,
  removeConfig,
  upsertConfig,
  getExtensions as apiGetExtensions,
  addExtension as apiAddExtension,
  removeExtension as apiRemoveExtension,
  providers,
  getProviderModels as apiGetProviderModels,
} from '../api';
import { syncBundledExtensions } from './settings/extensions';
import { userActionHeaders } from '../utils/userAction';
import {
  isCapabilityDefaultEnabled,
  shouldDefaultEnableAgentDrafter,
  shouldDefaultEnablePromotedCapability,
} from './settings/capabilities/capabilities';
import type {
  ConfigResponse,
  UpsertConfigQuery,
  ConfigKeyQuery,
  ExtensionResponse,
  ProviderDetails,
  ExtensionQuery,
  ExtensionConfig,
} from '../api';

export type { ExtensionConfig } from '../api/types.gen';

// Define a local version that matches the structure of the imported one
export type FixedExtensionEntry = ExtensionConfig & {
  enabled: boolean;
};

interface ConfigContextType {
  config: ConfigResponse['config'];
  providersList: ProviderDetails[];
  extensionsList: FixedExtensionEntry[];
  extensionWarnings: string[];
  upsert: (key: string, value: unknown, is_secret: boolean) => Promise<void>;
  /**
   * Re-read the whole config from the daemon.
   *
   * `config` is a cache, and `upsert` is not the only way its keys get written:
   * `setConfigProvider` writes BIOROUTER_PROVIDER/BIOROUTER_MODEL straight to
   * the API, and the CLI or another window can write any key at any time. Every
   * such write must be followed by this, or the cache goes on serving the
   * pre-write snapshot to every consumer — silently, and for as long as it
   * takes some unrelated `upsert` to refresh it (issue #52).
   *
   * Rejects if the re-read fails, leaving the cache exactly as it was — a
   * stale snapshot is bad, an erased one is worse. Callers that only wanted
   * the refresh as bookkeeping after a write of their own should catch and log
   * rather than report their write as failed.
   */
  refreshConfig: () => Promise<void>;
  read: (key: string, is_secret: boolean) => Promise<unknown>;
  remove: (key: string, is_secret: boolean) => Promise<void>;
  addExtension: (name: string, config: ExtensionConfig, enabled: boolean) => Promise<void>;
  toggleExtension: (name: string) => Promise<void>;
  removeExtension: (name: string) => Promise<void>;
  getProviders: (b: boolean) => Promise<ProviderDetails[]>;
  getExtensions: (b: boolean) => Promise<FixedExtensionEntry[]>;
  getProviderModels: (providerName: string) => Promise<string[]>;
  disableAllExtensions: () => Promise<void>;
  enableBotExtensions: (extensions: ExtensionConfig[]) => Promise<void>;
}

interface ConfigProviderProps {
  children: React.ReactNode;
}

export class MalformedConfigError extends Error {
  constructor() {
    super('Check contents of ~/.config/biorouter/config.yaml');
    this.name = 'MalformedConfigError';
    Object.setPrototypeOf(this, MalformedConfigError.prototype);
  }
}

const ConfigContext = createContext<ConfigContextType | undefined>(undefined);

export const ConfigProvider: React.FC<ConfigProviderProps> = ({ children }) => {
  const [config, setConfig] = useState<ConfigResponse['config']>({});
  const [providersList, setProvidersList] = useState<ProviderDetails[]>([]);
  const [extensionsList, setExtensionsList] = useState<FixedExtensionEntry[]>([]);
  const [extensionWarnings, setExtensionWarnings] = useState<string[]>([]);
  const configReadTicket = useRef(0);
  const appliedConfigRead = useRef(0);

  /**
   * Re-read the whole config into the cache. The only place that writes it.
   *
   * Two things this must never do, both learned the hard way:
   *
   * 1. **Empty the cache because the read failed.** `readAllConfig` is
   *    non-throwing by default, so an HTTP 500 *resolves* with `data`
   *    undefined — indistinguishable, to a `response.data?.config || {}`, from
   *    a config that really is empty. The cache is the only copy the UI has;
   *    losing it makes a fully configured app look unconfigured. So: ask the
   *    client to throw, treat a missing body as a failure too, and on any
   *    failure leave the previous snapshot exactly where it was.
   * 2. **Let an older read win.** Reads overlap — the mount read, a refresh
   *    after an API-mediated provider write, an unrelated `upsert` — and they
   *    can complete in any order. A read issued *before* a write that lands
   *    *after* the refresh which followed it would republish the pre-write
   *    snapshot, which is issue #52 all over again as a race. Each read takes
   *    a ticket on the way out and applies only if nothing newer has already
   *    been applied.
   *
   *    Note the rule is "newer than what was applied", not "the newest issued":
   *    a *failed* newer read applies nothing, and must not therefore condemn an
   *    older successful one — that would leave the cache empty on a race whose
   *    only successful read had the answer in hand.
   *
   * Rejects when the config could not be re-read, so callers can decide what
   * that means to them (see `reloadConfigAfterWrite`).
   */
  const reloadConfig = useCallback(async () => {
    const ticket = ++configReadTicket.current;

    const response = await readAllConfig({ throwOnError: true });

    const nextConfig = response.data?.config;
    if (!nextConfig) {
      throw new Error('The config read returned no configuration');
    }

    if (ticket < appliedConfigRead.current) {
      // A read issued after this one has already published a newer snapshot.
      return;
    }

    appliedConfigRead.current = ticket;
    setConfig(nextConfig);
  }, []);

  /**
   * Refresh the cache after a write that already succeeded.
   *
   * The write is the caller's outcome; the re-read is bookkeeping. Failing an
   * upsert because a *cache* could not be re-read reports the wrong thing in
   * the more alarming direction, and worse, a caller writing several keys in
   * sequence (`ProviderGuard` writes an API key, then the provider) would stop
   * partway through and leave a half-configured provider behind. If the daemon
   * is genuinely unreachable, the write itself has already failed and said so.
   */
  const reloadConfigAfterWrite = useCallback(async () => {
    try {
      await reloadConfig();
    } catch (error) {
      console.error('Failed to refresh the cached config after a write:', error);
    }
  }, [reloadConfig]);

  const upsert = useCallback(
    async (key: string, value: unknown, isSecret: boolean = false) => {
      const query: UpsertConfigQuery = {
        key: key,
        value: value,
        is_secret: isSecret,
      };
      await upsertConfig({
        body: query,
        // Issue #56 DR-16: this is the GUI's ONLY path to `/config/upsert`, and
        // real settings screens write capability keys through it —
        // BIOROUTER_LEAD_MODEL / BIOROUTER_LEAD_PROVIDER from Lead/Worker
        // settings, OLLAMA_HOST and LLAMACPP_EXTERNAL_HOST from the provider
        // forms. Those are the user editing their own settings; the daemon
        // guards the same four keys against a model curling the route, and
        // without this header it could not tell the two apart.
        headers: await userActionHeaders(),
      });
      await reloadConfigAfterWrite();
    },
    [reloadConfigAfterWrite]
  );

  const read = useCallback(async (key: string, is_secret: boolean = false) => {
    const query: ConfigKeyQuery = { key: key, is_secret: is_secret };
    const response = await readConfig({
      body: query,
    });
    return response.data;
  }, []);

  const remove = useCallback(
    async (key: string, is_secret: boolean) => {
      const query: ConfigKeyQuery = { key: key, is_secret: is_secret };
      await removeConfig({
        body: query,
        // Issue #56 DR-16: the same guard as `upsert`, because a DELETE of a
        // capability key restores its default and `OLLAMA_HOST`'s default is
        // loopback — i.e. Private. This is the GUI's only path to
        // `/config/remove`, and clearing a provider's host from the provider
        // form comes through here.
        headers: await userActionHeaders(),
      });
      await reloadConfigAfterWrite();
    },
    [reloadConfigAfterWrite]
  );

  const refreshExtensions = useCallback(async () => {
    const result = await apiGetExtensions();

    if (result.response.status === 422) {
      throw new MalformedConfigError();
    }

    if (result.error && !result.data) {
      console.log(result.error);
      return extensionsList;
    }

    const extensionResponse: ExtensionResponse = result.data!;
    setExtensionsList(extensionResponse.extensions);
    setExtensionWarnings(extensionResponse.warnings || []);
    return extensionResponse.extensions;
  }, [extensionsList]);

  const addExtension = useCallback(
    async (name: string, config: ExtensionConfig, enabled: boolean) => {
      const query: ExtensionQuery = { name, config, enabled };
      await apiAddExtension({
        body: query,
      });
      await reloadConfigAfterWrite();
      // Refresh extensions list after successful addition
      await refreshExtensions();
    },
    [reloadConfigAfterWrite, refreshExtensions]
  );

  const removeExtension = useCallback(
    async (name: string) => {
      await apiRemoveExtension({ path: { name: name } });
      await reloadConfigAfterWrite();
      // Refresh extensions list after successful removal
      await refreshExtensions();
    },
    [reloadConfigAfterWrite, refreshExtensions]
  );

  const getExtensions = useCallback(
    async (forceRefresh = false): Promise<FixedExtensionEntry[]> => {
      if (forceRefresh || extensionsList.length === 0) {
        return await refreshExtensions();
      }
      return extensionsList;
    },
    [extensionsList, refreshExtensions]
  );

  const toggleExtension = useCallback(
    async (name: string) => {
      const exts = await getExtensions(true);
      const extension = exts.find((ext) => ext.name === name);

      if (extension) {
        await addExtension(name, extension, !extension.enabled);
      }
    },
    [addExtension, getExtensions]
  );

  const getProviders = useCallback(
    async (forceRefresh = false): Promise<ProviderDetails[]> => {
      if (forceRefresh || providersList.length === 0) {
        try {
          const response = await providers();
          const providersData = response.data || [];
          setProvidersList(providersData);
          return providersData;
        } catch (error) {
          console.error('Failed to fetch providers:', error);
          return [];
        }
      }
      return providersList;
    },
    [providersList]
  );

  const getProviderModels = useCallback(async (providerName: string): Promise<string[]> => {
    try {
      const response = await apiGetProviderModels({
        path: { name: providerName },
        throwOnError: true,
      });
      return response.data || [];
    } catch (error) {
      console.error(`Failed to fetch models for provider ${providerName}:`, error);
      return [];
    }
  }, []);

  useEffect(() => {
    // Load all configuration data and providers on mount
    (async () => {
      // Load config through the same reader every later refresh uses, so there
      // is one set of rules about failures and ordering rather than two.
      try {
        await reloadConfig();
      } catch (error) {
        // The config is one of three independent loads here. Letting its
        // failure escape took providers and extensions down with it and left
        // an unhandled rejection behind; the cache simply stays empty until a
        // later refresh succeeds.
        console.error('Failed to load config:', error);
      }

      // Load providers
      try {
        const providersResponse = await providers();
        const providersData = providersResponse.data || [];
        setProvidersList(providersData);
      } catch (error) {
        console.error('Failed to load providers:', error);
        setProvidersList([]);
      }

      // Load extensions
      try {
        const extensionsResponse = await apiGetExtensions();
        let extensions = extensionsResponse.data?.extensions || [];

        // Always sync from bundled-extensions.json so new built-ins added across
        // versions get picked up automatically. syncBundledExtensions is idempotent —
        // it skips bundled extensions already present in the user's config.
        const addExtensionForSync = async (
          name: string,
          config: ExtensionConfig,
          enabled: boolean
        ) => {
          const query: ExtensionQuery = { name, config, enabled };
          await apiAddExtension({ body: query });
        };
        await syncBundledExtensions(extensions, addExtensionForSync);

        const capabilityMigrations = [
          {
            flag: 'biorouter.capabilities.defaultEnabled.v1',
            shouldEnable: (ext: FixedExtensionEntry) =>
              !ext.enabled && isCapabilityDefaultEnabled(ext),
          },
          {
            flag: 'biorouter.capabilities.promotedDefaults.v2',
            shouldEnable: shouldDefaultEnablePromotedCapability,
          },
          {
            flag: 'biorouter.capabilities.agentDrafterDefault.v3',
            shouldEnable: shouldDefaultEnableAgentDrafter,
          },
        ];

        for (const migration of capabilityMigrations) {
          if (localStorage.getItem(migration.flag)) continue;

          try {
            const current = (await apiGetExtensions()).data?.extensions || [];
            for (const ext of current) {
              if (!migration.shouldEnable(ext)) continue;

              const { enabled: _omit, ...cfg } = ext;
              await addExtensionForSync(ext.name, cfg as ExtensionConfig, true);
            }
          } catch (e) {
            console.error('Capability default-enable migration failed:', e);
          }
          localStorage.setItem(migration.flag, '1');
        }

        const refreshedResponse = await apiGetExtensions();
        extensions = refreshedResponse.data?.extensions || [];

        setExtensionsList(extensions);
        setExtensionWarnings(extensionsResponse.data?.warnings || []);
      } catch (error) {
        console.error('Failed to load extensions:', error);
      }
    })();
  }, [reloadConfig]);

  const contextValue = useMemo(() => {
    const disableAllExtensions = async () => {
      const currentExtensions = await getExtensions(true);
      for (const ext of currentExtensions) {
        if (ext.enabled) {
          await addExtension(ext.name, ext, false);
        }
      }
      await reloadConfigAfterWrite();
    };

    const enableBotExtensions = async (extensions: ExtensionConfig[]) => {
      for (const ext of extensions) {
        await addExtension(ext.name, ext, true);
      }
      await reloadConfigAfterWrite();
    };

    return {
      config,
      providersList,
      extensionsList,
      extensionWarnings,
      upsert,
      refreshConfig: reloadConfig,
      read,
      remove,
      addExtension,
      removeExtension,
      toggleExtension,
      getProviders,
      getExtensions,
      getProviderModels,
      disableAllExtensions,
      enableBotExtensions,
    };
  }, [
    config,
    providersList,
    extensionsList,
    extensionWarnings,
    upsert,
    read,
    remove,
    addExtension,
    removeExtension,
    toggleExtension,
    getProviders,
    getExtensions,
    getProviderModels,
    reloadConfig,
    reloadConfigAfterWrite,
  ]);

  return <ConfigContext.Provider value={contextValue}>{children}</ConfigContext.Provider>;
};

export const useConfig = () => {
  const context = useContext(ConfigContext);
  if (context === undefined) {
    throw new Error('useConfig must be used within a ConfigProvider');
  }
  return context;
};
