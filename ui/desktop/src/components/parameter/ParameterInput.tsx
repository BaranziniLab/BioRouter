import React from 'react';
import { AlertTriangle, Trash2, ChevronDown, ChevronRight } from '../icons/app-icons';
import { Parameter } from '../../workflow';

interface ParameterInputProps {
  parameter: Parameter;
  onChange: (name: string, updatedParameter: Partial<Parameter>) => void;
  onDelete?: (parameterKey: string) => void;
  isUnused?: boolean;
  isExpanded?: boolean;
  onToggleExpanded?: (parameterKey: string) => void;
}

const labelCls = 'block text-xs font-medium text-text-muted uppercase tracking-wider mb-1';
const inputCls =
  'w-full px-3 py-2 text-sm border border-border-subtle rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150';
const selectCls =
  'w-full px-3 py-2 text-sm border border-border-subtle rounded-lg bg-background-default text-text-default focus:outline-none focus:border-border-strong transition-colors duration-150';

const ParameterInput: React.FC<ParameterInputProps> = ({
  parameter,
  onChange,
  onDelete,
  isUnused = false,
  isExpanded = true,
  onToggleExpanded,
}) => {
  const { key, description, requirement } = parameter;
  const defaultValue = parameter.default || '';

  const handleToggleExpanded = (e: React.MouseEvent) => {
    if (onToggleExpanded && !(e.target as HTMLElement).closest('button')) {
      onToggleExpanded(key);
    }
  };

  return (
    <div className="my-2 border border-border-subtle rounded-xl bg-background-default overflow-hidden">
      {/* Header row */}
      <div
        className={`flex items-center justify-between px-4 py-3 ${onToggleExpanded ? 'cursor-pointer hover:bg-background-muted' : ''} transition-colors duration-150`}
        onClick={handleToggleExpanded}
      >
        <div className="flex items-center gap-2 flex-1 min-w-0">
          {onToggleExpanded && (
            <button
              type="button"
              className="p-0.5 text-text-muted hover:text-text-default rounded transition-colors flex-shrink-0"
              onClick={(e) => {
                e.stopPropagation();
                onToggleExpanded(key);
              }}
            >
              {isExpanded ? (
                <ChevronDown className="w-3.5 h-3.5" />
              ) : (
                <ChevronRight className="w-3.5 h-3.5" />
              )}
            </button>
          )}

          <code className="text-xs font-mono bg-background-medium px-2 py-0.5 rounded text-text-default">
            {'{{'}{key}{'}}'}
          </code>

          {isUnused && (
            <div
              className="flex items-center gap-1"
              title="This parameter is not referenced in instructions, prompt, or activities."
            >
              <AlertTriangle className="w-3.5 h-3.5 text-orange-500" />
              <span className="text-xs text-orange-500">Unused</span>
            </div>
          )}
        </div>

        {onDelete && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onDelete(key);
            }}
            className="p-1 text-text-muted hover:text-red-500 dark:hover:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 rounded transition-colors flex-shrink-0"
            title={`Delete ${key}`}
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
        )}
      </div>

      {/* Expanded detail */}
      {isExpanded && (
        <div className="px-4 pb-4 pt-3 border-t border-border-subtle space-y-3">
          {/* Description */}
          <div>
            <label className={labelCls}>Description</label>
            <input
              type="text"
              value={description || ''}
              onChange={(e) => onChange(key, { description: e.target.value })}
              className={inputCls}
              placeholder={`Prompt shown to the user for "${key}"`}
            />
            <p className="text-xs text-text-muted mt-1">Shown to the end-user when running the workflow.</p>
          </div>

          {/* Controls row */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={labelCls}>Input Type</label>
              <select
                className={selectCls}
                value={parameter.input_type || 'string'}
                onChange={(e) => onChange(key, { input_type: e.target.value as Parameter['input_type'] })}
              >
                <option value="string">String</option>
                <option value="select">Select</option>
                <option value="number">Number</option>
                <option value="boolean">Boolean</option>
              </select>
            </div>

            <div>
              <label className={labelCls}>Requirement</label>
              <select
                className={selectCls}
                value={requirement}
                onChange={(e) => onChange(key, { requirement: e.target.value as Parameter['requirement'] })}
              >
                <option value="required">Required</option>
                <option value="optional">Optional</option>
              </select>
            </div>
          </div>

          {/* Default value — only for optional */}
          {requirement === 'optional' && (
            <div>
              <label className={labelCls}>Default Value</label>
              <input
                type="text"
                value={defaultValue}
                onChange={(e) => onChange(key, { default: e.target.value })}
                className={inputCls}
                placeholder="Value used when the user leaves this blank"
              />
            </div>
          )}

          {/* Options — only for select type */}
          {parameter.input_type === 'select' && (
            <div>
              <label className={labelCls}>Options (one per line)</label>
              <textarea
                value={(parameter.options || []).join('\n')}
                onChange={(e) => onChange(key, { options: e.target.value.split('\n') })}
                onKeyDown={(e) => { if (e.key === 'Enter') e.stopPropagation(); }}
                className={`${inputCls} resize-none`}
                placeholder={'Option 1\nOption 2\nOption 3'}
                rows={3}
              />
              <p className="text-xs text-text-muted mt-1">Each line becomes a dropdown choice.</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
};

export default ParameterInput;
