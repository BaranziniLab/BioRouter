import { useEffect, useState } from 'react';

export interface ResponseStyle {
  key: string;
  label: string;
  description: string;
}

export const all_response_styles: ResponseStyle[] = [
  {
    key: 'detailed',
    label: 'Detailed',
    description: 'Tool calls are by default shown open to expose details',
  },
  {
    key: 'concise',
    label: 'Concise',
    description: 'Tool calls are by default closed and only show the tool used',
  },
];

interface ResponseStyleSelectionItemProps {
  currentStyle: string;
  style: ResponseStyle;
  showDescription: boolean;
  handleStyleChange: (newStyle: string) => void;
}

export function ResponseStyleSelectionItem({
  currentStyle,
  style,
  showDescription,
  handleStyleChange,
}: ResponseStyleSelectionItemProps) {
  const [checked, setChecked] = useState(currentStyle === style.key);

  useEffect(() => {
    setChecked(currentStyle === style.key);
  }, [currentStyle, style.key]);

  return (
    <div className="group hover:cursor-pointer text-sm">
      <div
        className={`flex items-center justify-between text-text-default py-3 px-2 -mx-2 rounded-lg cursor-pointer transition-colors duration-150 ${checked ? 'bg-background-medium' : 'hover:bg-background-medium'}`}
        onClick={() => handleStyleChange(style.key)}
      >
        <div className="flex">
          <div>
            <p className="text-sm font-medium text-text-default">{style.label}</p>
            {showDescription && (
              <p className="text-xs text-text-muted mt-0.5">{style.description}</p>
            )}
          </div>
        </div>

        <div className="relative flex items-center gap-2">
          <input
            type="radio"
            name="responseStyles"
            value={style.key}
            checked={checked}
            onChange={() => handleStyleChange(style.key)}
            className="peer sr-only"
          />
          <div
            className="h-4 w-4 rounded-full border border-neutral-300
                  peer-checked:border-[6px] peer-checked:border-black dark:peer-checked:border-white
                  peer-checked:bg-white dark:peer-checked:bg-black
                  transition-all duration-200 ease-in-out group-hover:border-neutral-400"
          ></div>
        </div>
      </div>
    </div>
  );
}
