export type LoadingStatus = 'loading' | 'success' | 'error';

// `size` is the rendered diameter in px (design §4.16: an 8px circle by default).
export default function Dot({
  size = 8,
  loadingStatus,
}: {
  size?: number;
  loadingStatus: LoadingStatus;
}) {
  const backgroundColorClasses = {
    loading: 'bg-background-warning',
    success: 'bg-background-success',
    error: 'bg-background-danger',
  };

  return (
    <div className="flex items-center justify-center">
      <div
        role="img"
        aria-label={loadingStatus}
        className={`rounded-full ${backgroundColorClasses[loadingStatus] || 'bg-background-strong'}`}
        style={{
          width: `${size}px`,
          height: `${size}px`,
        }}
      />
    </div>
  );
}
