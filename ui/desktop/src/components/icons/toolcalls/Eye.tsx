export const Eye = ({ className }: { className?: string }) => (
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
      d="M1.375 5.5C1.375 4.36091 2.29841 3.4375 3.4375 3.4375H7.5625C8.70159 3.4375 9.625 4.36091 9.625 5.5V5.5C9.625 6.63909 8.70159 7.5625 7.5625 7.5625H3.4375C2.29841 7.5625 1.375 6.63909 1.375 5.5V5.5ZM6.53125 5.5C6.53125 6.06954 6.06954 6.53125 5.5 6.53125C4.93046 6.53125 4.46875 6.06954 4.46875 5.5C4.46875 4.93046 4.93046 4.46875 5.5 4.46875C6.06954 4.46875 6.53125 4.93046 6.53125 5.5Z"
      fill="currentColor"
    />
  </svg>
);
