import { ModeSection } from '../mode/ModeSection';
import { ResponseStylesSection } from '../response_styles/ResponseStylesSection';
import { CapabilitiesSection } from '../capabilities/CapabilitiesSection';
import { BrsdkSection } from '../brsdk/BrsdkSection';
import { BioRouterHintsSection } from './BioRouterHintsSection';
import { SpellcheckToggle } from './SpellcheckToggle';
import MemorySection from '../memory/MemorySection';

export default function ChatSettingsSection() {
  return (
    <div className="pb-8">
      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider mb-1">
            Mode
          </h2>
          <p className="text-xs text-text-muted">
            Configure how Biorouter interacts with tools and extensions
          </p>
        </div>
        <div className="biorouter-settings-list">
          <ModeSection />
        </div>
      </div>

      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider mb-1">
            Response Styles
          </h2>
          <p className="text-xs text-text-muted">
            Choose how Biorouter should format and style its responses
          </p>
        </div>
        <div className="biorouter-settings-list">
          <ResponseStylesSection />
        </div>
      </div>

      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider mb-1">
            Capabilities
          </h2>
          <p className="text-xs text-text-muted">
            Built-in abilities are managed here. Most are enabled by default.
          </p>
        </div>
        <div className="biorouter-settings-list">
          <CapabilitiesSection />
        </div>
      </div>

      {/* Directly under Capabilities, which owns the switch that turns memory
          on and off: the store and its own toggle belong together. */}
      <MemorySection />

      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider mb-1">
            App SDK
          </h2>
          <p className="text-xs text-text-muted">
            Opt-in safety frameworks for Agent-Drafter apps. All are off by default and apply only
            to Agent-Drafter apps, never to normal chat.
          </p>
        </div>
        <div className="biorouter-settings-list">
          <BrsdkSection />
        </div>
      </div>

      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
            Editor
          </h2>
        </div>
        <div className="biorouter-settings-list">
          <SpellcheckToggle />
        </div>
      </div>

      <div className="biorouter-settings-section">
        <div className="biorouter-settings-section-header">
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
            Project
          </h2>
        </div>
        <div className="biorouter-settings-list">
          <BioRouterHintsSection />
        </div>
      </div>
    </div>
  );
}
