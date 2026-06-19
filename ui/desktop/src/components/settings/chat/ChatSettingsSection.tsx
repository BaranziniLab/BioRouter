import { ModeSection } from '../mode/ModeSection';
import { ResponseStylesSection } from '../response_styles/ResponseStylesSection';
import { BioRouterHintsSection } from './BioRouterHintsSection';
import { SpellcheckToggle } from './SpellcheckToggle';

export default function ChatSettingsSection() {
  return (
    <div className="space-y-8 pb-8">
      <div>
        <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-1">Mode</h2>
        <p className="text-xs text-text-muted mb-4">
          Configure how BioRouter interacts with tools and extensions
        </p>
        <div className="border-t border-border-subtle pt-2">
          <ModeSection />
        </div>
      </div>

      <div>
        <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-1">
          Response Styles
        </h2>
        <p className="text-xs text-text-muted mb-4">
          Choose how BioRouter should format and style its responses
        </p>
        <div className="border-t border-border-subtle pt-2">
          <ResponseStylesSection />
        </div>
      </div>

      <div>
        <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3">
          Editor
        </h2>
        <div className="border-t border-border-subtle divide-y divide-border-subtle">
          <SpellcheckToggle />
        </div>
      </div>

      <div>
        <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3">
          Project
        </h2>
        <div className="border-t border-border-subtle divide-y divide-border-subtle">
          <BioRouterHintsSection />
        </div>
      </div>
    </div>
  );
}
