export const Camera = ({ className }: { className?: string }) => (
  <svg
    width="11"
    height="11"
    viewBox="0 0 11 11"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    className={className}
  >
    <rect width="11" height="11" rx="2" fill="currentColor" fillOpacity={0.2} />
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M1.375 3.4375C1.375 3.0578 1.6828 2.75 2.0625 2.75H8.9375C9.3172 2.75 9.625 3.0578 9.625 3.4375V7.5625C9.625 7.9422 9.3172 8.25 8.9375 8.25H2.0625C1.6828 8.25 1.375 7.9422 1.375 7.5625V3.4375ZM6.53125 5.5C6.53125 6.06954 6.06954 6.53125 5.5 6.53125C4.93046 6.53125 4.46875 6.06954 4.46875 5.5C4.46875 4.93046 4.93046 4.46875 5.5 4.46875C6.06954 4.46875 6.53125 4.93046 6.53125 5.5Z"
      fill="currentColor"
    />
  </svg>
);
