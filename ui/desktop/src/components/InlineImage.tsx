import { useState, useEffect } from 'react';

type TempPathProps = { kind: 'temp-path'; path: string; alt?: string; className?: string };
type DataProps = { kind: 'data'; data: string; mimeType: string; alt?: string; className?: string };
export type InlineImageProps = TempPathProps | DataProps;

export function InlineImage(props: InlineImageProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const [error, setError] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [imageData, setImageData] = useState<string | null>(null);

  const alt = props.alt ?? 'Image';
  const className = props.className ?? '';

  // Stable key that changes when the image source changes, regardless of kind.
  const kind = props.kind;
  const sourceKey = kind === 'data' ? props.data : props.path;
  const mimeType = kind === 'data' ? props.mimeType : null;

  useEffect(() => {
    let cancelled = false;

    if (kind === 'data') {
      const dataUrl = `data:${mimeType};base64,${sourceKey}`;
      setImageData(dataUrl);
      setIsLoading(false);
      setError(false);
      return;
    }

    // kind === 'temp-path'
    setImageData(null);
    setIsLoading(true);
    setError(false);

    const loadImage = async () => {
      try {
        const data = await window.electron.getTempImage(sourceKey);
        if (cancelled) return;
        if (data) {
          setImageData(data);
          setIsLoading(false);
        } else {
          setError(true);
          setIsLoading(false);
        }
      } catch (err) {
        console.error('Error loading image:', err);
        if (!cancelled) {
          setError(true);
          setIsLoading(false);
        }
      }
    };

    loadImage();

    return () => {
      cancelled = true;
    };
  }, [kind, sourceKey, mimeType]);

  const handleError = () => {
    setError(true);
    setIsLoading(false);
  };

  const toggleExpand = () => {
    if (!error) {
      setIsExpanded(!isExpanded);
    }
  };

  // For temp-path, validate it is a safe file path
  if (props.kind === 'temp-path' && !props.path.includes('biorouter-pasted-images')) {
    return (
      <div className="text-text-danger text-xs italic mt-1 mb-1">
        Invalid image path: {props.path}
      </div>
    );
  }

  if (error) {
    const label = props.kind === 'temp-path' ? props.path : `${props.mimeType} image`;
    return (
      <div className="text-text-danger text-xs italic mt-1 mb-1">Unable to load image: {label}</div>
    );
  }

  return (
    <div className={`image-preview mt-2 mb-2 ${className}`}>
      {isLoading && (
        <div className="animate-pulse bg-background-medium rounded w-40 h-40 flex items-center justify-center">
          <span className="text-text-muted text-xs">Loading...</span>
        </div>
      )}
      {imageData && (
        <button
          type="button"
          onClick={toggleExpand}
          className={`block rounded transition-opacity hover:opacity-90 ${isExpanded ? 'max-w-full' : 'max-w-40 max-h-40'} ${isLoading ? 'hidden' : ''}`}
          title={isExpanded ? 'Collapse image' : 'Expand image'}
          aria-label={isExpanded ? 'Collapse image' : 'Expand image'}
        >
          <img
            src={imageData}
            alt={alt}
            onError={handleError}
            className={
              isExpanded ? 'max-h-96 max-w-full object-contain' : 'max-h-40 max-w-40 object-contain'
            }
          />
        </button>
      )}
    </div>
  );
}
