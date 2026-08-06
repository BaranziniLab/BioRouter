import { workflowNameSchema, WORKFLOW_NAME_PLACEHOLDER } from './workflowNameUtils';

interface WorkflowNameFieldProps {
  id: string;
  value: string;
  onChange: (value: string) => void;
  onBlur: () => void;
  errors: string[];
  label?: string;
  required?: boolean;
  disabled?: boolean;
}

export function WorkflowNameField({
  id,
  value,
  onChange,
  onBlur,
  errors,
  label = 'Workflow Name',
  required = true,
  disabled = false,
}: WorkflowNameFieldProps) {
  return (
    <div>
      <label htmlFor={id} className="block text-label text-text-default mb-2">
        {label} {required && <span className="text-text-danger">*</span>}
      </label>
      <input
        id={id}
        type="text"
        value={value}
        onChange={(e) => {
          // Allow typing normally, only filter out invalid characters but keep spaces
          const rawValue = e.target.value;
          const filtered = rawValue.replace(/[^a-zA-Z0-9\s-]/g, '');
          onChange(filtered);
        }}
        onBlur={(e) => {
          // Transform on blur: convert to lowercase and replace spaces with dashes
          const rawValue = e.target.value;
          const transformed = rawValue
            .toLowerCase()
            .replace(/\s+/g, '-')
            .replace(/[^a-z0-9-]/g, '')
            .replace(/-+/g, '-')
            .replace(/^-+|-+$/g, '');

          onChange(transformed);
          onBlur();
        }}
        disabled={disabled}
        className={`w-full p-3 border rounded-element bg-background-default text-text-default ${errors.length > 0 ? 'border-border-danger' : 'border-border-subtle'} ${disabled ? 'opacity-50 cursor-not-allowed' : ''}`}
        placeholder={WORKFLOW_NAME_PLACEHOLDER}
        data-testid="workflow-name-input"
      />
      <p className="text-supporting text-text-muted mt-1">
        Will be automatically formatted (lowercase, dashes for spaces)
      </p>
      {errors.length > 0 && <p className="text-text-danger text-body mt-1">{errors[0]}</p>}
    </div>
  );
}

export { workflowNameSchema };
