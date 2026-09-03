import { Input } from '../../../ui/input';

interface ExtensionConfigFieldsProps {
  type: 'stdio' | 'sse' | 'streamable_http' | 'builtin';
  full_cmd: string;
  endpoint: string;
  onChange: (key: string, value: string) => void;
  submitAttempted?: boolean;
  isValid?: boolean;
}

export default function ExtensionConfigFields({
  type,
  full_cmd,
  endpoint,
  onChange,
  submitAttempted = false,
  isValid,
}: ExtensionConfigFieldsProps) {
  if (type === 'stdio') {
    return (
      <div className="space-y-4">
        <div>
          <label className="text-sm font-medium mb-2 block text-text-default">Command</label>
          <div className="relative">
            {/* Monospace, because this is a COMMAND — the same value
                ExtensionItem.tsx:137 prints in `font-mono` on the row this
                dialog edits. Body font here meant a command was monospace to
                read and proportional to edit, in a modal that opens directly
                over the row showing it.
                The endpoint field below is deliberately NOT changed: a URL is a
                different value class, and it is body font everywhere else too
                (e.g. LocalModelInventory's "Official URL"). */}
            <Input
              value={full_cmd}
              onChange={(e) => onChange('cmd', e.target.value)}
              placeholder="e.g. npx -y @modelcontextprotocol/my-extension <filepath>"
              className={`w-full font-mono ${!submitAttempted || isValid ? 'border-border-subtle' : 'border-border-danger'} text-text-default`}
            />
            {submitAttempted && !isValid && (
              <div className="absolute text-xs text-text-danger mt-1">Command is required</div>
            )}
          </div>
        </div>
      </div>
    );
  } else {
    return (
      <div>
        <label className="text-sm font-medium mb-2 block text-text-default">Endpoint</label>
        <div className="relative">
          <Input
            value={endpoint}
            onChange={(e) => onChange('endpoint', e.target.value)}
            placeholder="Enter endpoint URL..."
            className={`w-full ${!submitAttempted || isValid ? 'border-border-subtle' : 'border-border-danger'} text-text-default`}
          />
          {submitAttempted && !isValid && (
            <div className="absolute text-xs text-text-danger mt-1">Endpoint URL is required</div>
          )}
        </div>
      </div>
    );
  }
}
