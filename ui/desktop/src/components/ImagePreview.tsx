import { InlineImage } from './InlineImage';

interface ImagePreviewProps {
  src: string;
  alt?: string;
  className?: string;
}

export default function ImagePreview({ src, alt, className }: ImagePreviewProps) {
  return <InlineImage kind="temp-path" path={src} alt={alt} className={className} />;
}
