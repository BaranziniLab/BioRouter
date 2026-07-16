import { useState, useEffect, useRef } from 'react';
import { Switch } from '../../ui/switch';
import { Button } from '../../ui/button';
import { Settings } from '../../icons/app-icons';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '../../ui/dialog';
import UpdateSection from './UpdateSection';
import UsageSection from '../usage/UsageSection';
import ResetPanel from './ResetPanel';

import { COST_TRACKING_ENABLED, UPDATES_ENABLED } from '../../../updates';
import ThemeSelector from '../../BioRouterSidebar/ThemeSelector';
import ThemeFamilySelector from '../../BioRouterSidebar/ThemeFamilySelector';
import BlockLogoBlack from './icons/block-lockup_black.png';
import BlockLogoWhite from './icons/block-lockup_white.png';

interface AppSettingsSectionProps {
  scrollToSection?: string;
}

export default function AppSettingsSection({ scrollToSection }: AppSettingsSectionProps) {
  const [menuBarIconEnabled, setMenuBarIconEnabled] = useState(true);
  const [dockIconEnabled, setDockIconEnabled] = useState(true);
  const [wakelockEnabled, setWakelockEnabled] = useState(true);
  const [isMacOS, setIsMacOS] = useState(false);
  const [isDockSwitchDisabled, setIsDockSwitchDisabled] = useState(false);
  const [showNotificationModal, setShowNotificationModal] = useState(false);
  const [showPricing, setShowPricing] = useState(true);
  const [isDarkMode, setIsDarkMode] = useState(false);
  const [usageVersion, setUsageVersion] = useState(0);
  const updateSectionRef = useRef<HTMLDivElement>(null);

  const shouldShowUpdates = !window.appConfig.get('BIOROUTER_VERSION');

  useEffect(() => {
    setIsMacOS(window.electron.platform === 'darwin');
  }, []);

  useEffect(() => {
    const updateTheme = () => {
      setIsDarkMode(document.documentElement.classList.contains('dark'));
    };
    updateTheme();
    const observer = new MutationObserver(updateTheme);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const stored = localStorage.getItem('show_pricing');
    setShowPricing(stored !== 'false');
  }, []);

  useEffect(() => {
    if (scrollToSection === 'update' && updateSectionRef.current) {
      setTimeout(() => {
        updateSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }, 100);
    }
  }, [scrollToSection]);

  useEffect(() => {
    window.electron.getMenuBarIconState().then((enabled) => {
      setMenuBarIconEnabled(enabled);
    });

    window.electron.getWakelockState().then((enabled) => {
      setWakelockEnabled(enabled);
    });

    if (isMacOS) {
      window.electron.getDockIconState().then((enabled) => {
        setDockIconEnabled(enabled);
      });
    }
  }, [isMacOS]);

  const handleMenuBarIconToggle = async () => {
    const newState = !menuBarIconEnabled;
    if (!newState && !dockIconEnabled && isMacOS) {
      const success = await window.electron.setDockIcon(true);
      if (success) {
        setDockIconEnabled(true);
      }
    }
    const success = await window.electron.setMenuBarIcon(newState);
    if (success) {
      setMenuBarIconEnabled(newState);
    }
  };

  const handleDockIconToggle = async () => {
    const newState = !dockIconEnabled;
    if (!newState && !menuBarIconEnabled) {
      const success = await window.electron.setMenuBarIcon(true);
      if (success) {
        setMenuBarIconEnabled(true);
      }
    }
    setIsDockSwitchDisabled(true);
    setTimeout(() => {
      setIsDockSwitchDisabled(false);
    }, 1000);
    const success = await window.electron.setDockIcon(newState);
    if (success) {
      setDockIconEnabled(newState);
    }
  };

  const handleWakelockToggle = async () => {
    const newState = !wakelockEnabled;
    const success = await window.electron.setWakelock(newState);
    if (success) {
      setWakelockEnabled(newState);
    }
  };

  const handleShowPricingToggle = (checked: boolean) => {
    setShowPricing(checked);
    localStorage.setItem('show_pricing', String(checked));
    window.dispatchEvent(new CustomEvent('storage'));
  };

  return (
    <div className="pb-8">
      {/* Appearance */}
      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
            Appearance
          </h2>
        </div>
        <div className="biorouter-settings-list">
          <div className="biorouter-settings-row flex items-center justify-between px-3 py-2.5">
            <div>
              <p className="text-sm font-medium text-text-default">Notifications</p>
              <p className="text-xs text-text-muted mt-0.5 max-w-md">
                Notifications are managed by your OS.{' '}
                <span
                  className="underline cursor-pointer"
                  onClick={() => setShowNotificationModal(true)}
                >
                  Configuration guide
                </span>
              </p>
            </div>
            <Button
              variant="secondary"
              onClick={async () => {
                try {
                  await window.electron.openNotificationsSettings();
                } catch (error) {
                  console.error('Failed to open notification settings:', error);
                }
              }}
            >
              <Settings />
              Open Settings
            </Button>
          </div>

          <div className="biorouter-settings-row flex items-center justify-between px-3 py-2.5">
            <div>
              <p className="text-sm font-medium text-text-default">Menu bar icon</p>
              <p className="text-xs text-text-muted mt-0.5">Show Biorouter in the menu bar</p>
            </div>
            <Switch
              checked={menuBarIconEnabled}
              onCheckedChange={handleMenuBarIconToggle}
              variant="mono"
            />
          </div>

          {isMacOS && (
            <div className="biorouter-settings-row flex items-center justify-between px-3 py-2.5">
              <div>
                <p className="text-sm font-medium text-text-default">Dock icon</p>
                <p className="text-xs text-text-muted mt-0.5">Show Biorouter in the dock</p>
              </div>
              <Switch
                disabled={isDockSwitchDisabled}
                checked={dockIconEnabled}
                onCheckedChange={handleDockIconToggle}
                variant="mono"
              />
            </div>
          )}

          <div className="biorouter-settings-row flex items-center justify-between px-3 py-2.5">
            <div>
              <p className="text-sm font-medium text-text-default">Prevent Sleep</p>
              <p className="text-xs text-text-muted mt-0.5 max-w-md">
                Keep your computer awake while Biorouter is running a task (screen can still lock)
              </p>
            </div>
            <Switch
              checked={wakelockEnabled}
              onCheckedChange={handleWakelockToggle}
              variant="mono"
            />
          </div>

          {COST_TRACKING_ENABLED && (
            <div className="biorouter-settings-row flex items-center justify-between px-3 py-2.5">
              <div>
                <p className="text-sm font-medium text-text-default">Cost Tracking</p>
                <p className="text-xs text-text-muted mt-0.5">Show model pricing and usage costs</p>
              </div>
              <Switch
                checked={showPricing}
                onCheckedChange={handleShowPricingToggle}
                variant="mono"
              />
            </div>
          )}
        </div>
      </div>

      {/* Theme */}
      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider mb-1">
            Theme
          </h2>
          <p className="text-xs text-text-muted">Customize the look and feel of Biorouter</p>
        </div>
        <div className="flex flex-col gap-4">
          <div>
            <p className="text-xs text-text-muted mb-1.5">Palette</p>
            <div className="biorouter-settings-control-strip">
              <ThemeFamilySelector className="w-auto" hideTitle horizontal />
            </div>
          </div>
          <div>
            <p className="text-xs text-text-muted mb-1.5">Mode</p>
            <div className="biorouter-settings-control-strip">
              <ThemeSelector className="w-auto" hideTitle horizontal />
            </div>
          </div>
        </div>
      </div>

      {/* Usage — accumulated (billed) tokens + cost, month-to-date vs budget */}
      {COST_TRACKING_ENABLED && showPricing && <UsageSection key={usageVersion} />}

      <ResetPanel
        onReset={(categories) => {
          if (categories.includes('history')) setUsageVersion((version) => version + 1);
        }}
      />

      {/* Help & Feedback */}
      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider mb-1">
            Help &amp; Feedback
          </h2>
          <p className="text-xs text-text-muted">
            Help us improve Biorouter by reporting issues or requesting new features
          </p>
        </div>
        <div className="biorouter-settings-control-strip">
          <Button
            onClick={() => {
              window.open(
                'https://github.com/BaranziniLab/biorouter/issues/new?template=bug_report.md',
                '_blank'
              );
            }}
            variant="secondary"
          >
            Report a Bug
          </Button>
          <Button
            onClick={() => {
              window.open(
                'https://github.com/BaranziniLab/biorouter/issues/new?template=feature_request.md',
                '_blank'
              );
            }}
            variant="secondary"
          >
            Request a Feature
          </Button>
        </div>
      </div>

      {/* Version */}
      {!shouldShowUpdates && (
        <div className="biorouter-settings-section">
          <div className="biorouter-settings-section-header">
            <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
              Version
            </h2>
          </div>
          <div className="biorouter-settings-control-strip">
            <div className="flex items-center gap-3">
              <img
                src={isDarkMode ? BlockLogoWhite : BlockLogoBlack}
                alt="Block Logo"
                className="h-8 w-auto"
              />
              <span className="text-2xl font-mono text-text-default">
                {String(window.appConfig.get('BIOROUTER_VERSION') || 'Development')}
              </span>
            </div>
          </div>
        </div>
      )}

      {/* Updates */}
      {UPDATES_ENABLED && shouldShowUpdates && (
        <div ref={updateSectionRef} className="biorouter-settings-section">
          <div className="biorouter-settings-section-header">
            <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider mb-1">
              Updates
            </h2>
            <p className="text-xs text-text-muted">
              Check for and install updates to keep Biorouter running at its best
            </p>
          </div>
          <div className="biorouter-settings-control-strip">
            <UpdateSection />
          </div>
        </div>
      )}

      {/* Notification Instructions Modal */}
      <Dialog
        open={showNotificationModal}
        onOpenChange={(open) => !open && setShowNotificationModal(false)}
      >
        <DialogContent className="sm:max-w-[500px]">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Settings className="text-iconStandard" size={24} />
              How to Enable Notifications
            </DialogTitle>
          </DialogHeader>

          <div className="py-4">
            {isMacOS ? (
              <div className="space-y-4">
                <p>To enable notifications on macOS:</p>
                <ol className="list-decimal pl-5 space-y-2">
                  <li>Open System Preferences</li>
                  <li>Click on Notifications</li>
                  <li>Find and select Biorouter in the application list</li>
                  <li>Enable notifications and adjust settings as desired</li>
                </ol>
              </div>
            ) : (
              <div className="space-y-4">
                <p>To enable notifications on Windows:</p>
                <ol className="list-decimal pl-5 space-y-2">
                  <li>Open Settings</li>
                  <li>Go to System &gt; Notifications</li>
                  <li>Find and select Biorouter in the application list</li>
                  <li>Toggle notifications on and adjust settings as desired</li>
                </ol>
              </div>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowNotificationModal(false)}>
              Close
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
