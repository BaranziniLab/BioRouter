export type LoadingStatus = 'loading' | 'success' | 'error';
export default function Dot({
  size,
  loadingStatus,
}: {
  size: number;
  loadingStatus: LoadingStatus;
}) {
  const backgroundColorClasses = {
    loading: 'bg-background-info',
    success: 'bg-background-success',
    error: 'bg-background-danger',
  };

  return (
    <div className={`${loadingStatus === 'loading' ? '' : ''} flex items-center justify-center`}>
      <div
        className={`rounded-full ${backgroundColorClasses[loadingStatus] || 'bg-background-strong'}`}
        style={{
          width: `${size * 2}px`,
          height: `${size * 2}px`,
        }}
      />
    </div>
  );
}
