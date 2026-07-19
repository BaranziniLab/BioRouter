import { useEffect, useRef, useState } from 'react';
import { ChevronDown } from '../icons/app-icons';
import { Button } from '../ui/button';

interface WorkflowExpandableInfoProps {
  infoLabel: string;
  infoValue: string;
  required?: boolean;
  onClickEdit: () => void;
}

export default function WorkflowExpandableInfo({
  infoValue,
  infoLabel,
  required = false,
  onClickEdit,
}: WorkflowExpandableInfoProps) {
  const [isValueExpanded, setValueExpanded] = useState(false);
  const [isClamped, setIsClamped] = useState(false);
  const contentRef = useRef<HTMLParagraphElement>(null);
  const measureRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = measureRef.current;
    if (el) {
      const lineHeight = parseFloat(window.getComputedStyle(el).lineHeight || '0');
      const maxHeight = lineHeight * 3;
      const actualHeight = el.scrollHeight;
      setIsClamped(actualHeight > maxHeight);
    }
  }, [infoValue]);

  return (
    <>
      <div className="flex justify-between items-center mb-2">
        <label className="block text-sm text-text-default font-semibold">
          {infoLabel} {required && <span className="text-text-danger">*</span>}
        </label>
      </div>

      <div className="relative rounded-lg bg-background-default text-text-default">
        {infoValue && (
          <>
            <div
              ref={measureRef}
              className="invisible absolute whitespace-pre-wrap w-full pointer-events-none"
              style={{ position: 'absolute', top: '-9999px' }}
            >
              {infoValue}
            </div>

            <p
              ref={contentRef}
              className={`whitespace-pre-wrap transition-all duration-300 ${
                !isValueExpanded ? 'line-clamp-3' : ''
              }`}
            >
              {infoValue}
            </p>
          </>
        )}
        <div className="mt-4 flex items-center justify-between">
          <Button
            type="button"
            onClick={(e) => {
              e.preventDefault();
              setValueExpanded(true);
              onClickEdit();
            }}
            className="w-36 px-3 py-3 bg-background-accent text-sm text-text-on-accent rounded-xl hover:bg-background-accent/90 transition-colors"
          >
            {infoValue ? 'Edit' : 'Add'} {infoLabel.toLowerCase()}
          </Button>

          {infoValue && isClamped && (
            <Button
              type="button"
              variant="ghost"
              shape="round"
              onClick={() => setValueExpanded(!isValueExpanded)}
              aria-label={isValueExpanded ? 'Collapse content' : 'Expand content'}
              title={isValueExpanded ? 'Collapse' : 'Expand'}
              className="bg-background-muted hover:bg-background-default text-text-muted hover:text-text-default transition-colors"
            >
              <ChevronDown
                className={`w-6 h-6 transition-transform duration-300 ${
                  isValueExpanded ? 'rotate-180' : ''
                }`}
              />
            </Button>
          )}
        </div>
      </div>
    </>
  );
}
