/**
 * CustomRadio - A reusable radio button component with dark mode support
 * @param {Object} props - Component props
 * @param {string} props.id - Unique identifier for the radio input
 * @param {string} props.name - Name attribute for the radio input
 * @param {string} props.value - Value of the radio input
 * @param {boolean} props.checked - Whether the radio is checked
 * @param {function} props.onChange - Function to call when radio selection changes
 * @param {boolean} [props.disabled] - Whether the radio is disabled
 * @param {React.ReactNode} [props.label] - Primary label content
 * @param {React.ReactNode} [props.secondaryLabel] - Secondary/subtitle label content
 * @param {React.ReactNode} [props.rightContent] - Optional content to display on the right side
 * @param {string} [props.className] - Additional CSS classes for the main container
 * @returns {JSX.Element}
 */
const CustomRadio = ({
  id,
  name,
  value,
  checked,
  onChange,
  disabled = false,
  label = null,
  secondaryLabel = null,
  rightContent = null,
  className = '',
}: {
  id: string;
  name: string;
  value: string;
  checked: boolean;
  onChange: (event: React.ChangeEvent<HTMLInputElement>) => void;
  disabled?: boolean;
  label?: React.ReactNode;
  secondaryLabel?: React.ReactNode;
  rightContent?: React.ReactNode;
  className?: string;
}) => {
  return (
    <label
      htmlFor={id}
      className={`flex min-h-8 justify-between items-center py-2 cursor-pointer ${disabled ? 'opacity-50 cursor-not-allowed' : ''} ${className}`}
    >
      <div className="flex items-center">
        {/* 16px control. input, ring and dot share this box so both siblings react to peer-checked. */}
        <span className="relative mr-2 inline-flex h-4 w-4 shrink-0 items-center justify-center">
          <input
            type="radio"
            id={id}
            name={name}
            value={value}
            checked={checked}
            onChange={onChange}
            disabled={disabled}
            className="peer sr-only"
          />
          <span
            className="pointer-events-none absolute inset-0 rounded-full border-[1.5px] border-border-strong
                      transition-colors duration-[var(--motion-fast)] ease-[var(--ease-out)]
                      peer-checked:border-border-accent"
          />
          <span
            className="pointer-events-none h-1.5 w-1.5 rounded-full bg-background-accent opacity-0
                      transition-opacity duration-[var(--motion-fast)] ease-[var(--ease-out)]
                      peer-checked:opacity-100"
          />
        </span>

        {(label || secondaryLabel) && (
          <div>
            {label && <p className="text-sm text-text-default">{label}</p>}
            {secondaryLabel && <p className="text-xs text-text-muted">{secondaryLabel}</p>}
          </div>
        )}
      </div>

      {rightContent && (
        <div className="flex items-center text-sm text-text-muted">{rightContent}</div>
      )}
    </label>
  );
};

export default CustomRadio;
