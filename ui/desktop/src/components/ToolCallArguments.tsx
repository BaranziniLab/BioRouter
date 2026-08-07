import { useState } from 'react';
import MarkdownContent from './MarkdownContent';
import Expand from './ui/Expand';

export type ToolCallArgumentValue =
  | string
  | number
  | boolean
  | null
  | ToolCallArgumentValue[]
  | { [key: string]: ToolCallArgumentValue };

interface ToolCallArgumentsProps {
  /**
   * Parsed tool arguments. May be absent or a non-object when rendering a
   * *pending* tool call (§6.1b) whose arguments are still streaming and are not
   * yet valid JSON — this component must render nothing rather than throw in
   * that case.
   */
  args?: Record<string, ToolCallArgumentValue> | null;
}

/**
 * MONO FOR DATA — the app's own rule (P6), and this was the largest surface
 * still breaking it.
 *
 * A tool's arguments are file paths, ids, globs, SQL, JSON and shell fragments.
 * Set in the proportional UI face they were unreadable in the way that matters:
 * `l` and `1` and `I` collapse, a leading or trailing space in a path is
 * invisible, and two paths under one another do not line up so you cannot see
 * where they diverge. `text-code` is the same 13/20 the terminal and every code
 * block use, so an argument and the command it produced are set identically.
 *
 * The KEY column stays proportional: a key is a label naming the value, not the
 * value, and setting both in mono would remove the one cue that tells the two
 * columns apart.
 */
const ARG_KEY_CLASS = 'min-w-[140px] shrink-0 text-secondary text-text-muted';
const ARG_VALUE_CLASS = 'min-w-0 font-mono text-code text-text-muted';

export function ToolCallArguments({ args }: ToolCallArgumentsProps) {
  const [expandedKeys, setExpandedKeys] = useState<Record<string, boolean>>({});

  const toggleKey = (key: string) => {
    setExpandedKeys((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  const renderValue = (key: string, value: ToolCallArgumentValue) => {
    if (typeof value === 'string') {
      const needsExpansion = value.length > 60;
      const isExpanded = expandedKeys[key];

      if (!needsExpansion) {
        return (
          <div className="mb-2">
            <div className="flex flex-row">
              <span className={ARG_KEY_CLASS}>{key}</span>
              <span className={`${ARG_VALUE_CLASS} break-words`}>{value}</span>
            </div>
          </div>
        );
      }

      return (
        <div className={`mb-2 ${isExpanded ? '' : 'truncate min-w-0'}`}>
          <div className={`flex flex-row items-stretch ${isExpanded ? '' : 'truncate min-w-0'}`}>
            <button onClick={() => toggleKey(key)} className={`flex text-left ${ARG_KEY_CLASS}`}>
              <span>{key}</span>
            </button>
            <div className={`w-full flex items-stretch ${isExpanded ? '' : 'truncate min-w-0'}`}>
              {isExpanded ? (
                <div className="min-w-0">
                  <MarkdownContent content={value} className="text-secondary text-text-muted" />
                </div>
              ) : (
                <button
                  onClick={() => toggleKey(key)}
                  className={`text-left ${ARG_VALUE_CLASS} ${isExpanded ? '' : 'truncate'}`}
                >
                  {value}
                </button>
              )}
              <button
                onClick={() => toggleKey(key)}
                className="flex flex-row items-stretch grow text-text-muted pr-2"
              >
                <div className="min-w-2 grow" />
                <Expand size={5} isExpanded={isExpanded} />
              </button>
            </div>
          </div>
        </div>
      );
    }

    // Handle non-string values (arrays, objects, etc.)
    const content = Array.isArray(value)
      ? value.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join('\n')
      : typeof value === 'object' && value !== null
        ? JSON.stringify(value, null, 2)
        : String(value);

    return (
      <div className="mb-2">
        <div className="flex flex-row">
          <span className={ARG_KEY_CLASS}>{key}</span>
          <pre className={`${ARG_VALUE_CLASS} max-w-full overflow-x-auto whitespace-pre-wrap`}>
            {content}
          </pre>
        </div>
      </div>
    );
  };

  // Tolerate absent / non-object args (a pending tool call whose arguments have
  // not finished streaming, or unparseable partial JSON): render nothing.
  if (!args || typeof args !== 'object' || Array.isArray(args)) {
    return null;
  }

  return (
    <div className="my-2">
      {Object.entries(args).map(([key, value]) => (
        <div key={key}>{renderValue(key, value)}</div>
      ))}
    </div>
  );
}
