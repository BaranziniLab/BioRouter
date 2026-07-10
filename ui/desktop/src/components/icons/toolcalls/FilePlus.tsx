export const FilePlus = ({ className }: { className?: string }) => (
  <svg
    width="11"
    height="11"
    viewBox="0 0 11 11"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    className={className}
  >
    <rect width="11" height="11" rx="2" fill="currentColor" fillOpacity={0.2} />
    <rect x="2" y="5" width="7" height="1" rx="0.5" fill="currentColor" />
    <rect
      x="6"
      y="2"
      width="7"
      height="1"
      rx="0.5"
      transform="rotate(90 6 2)"
      fill="currentColor"
    />
  </svg>
);
