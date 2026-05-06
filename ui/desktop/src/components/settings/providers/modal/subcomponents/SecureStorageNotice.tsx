import { Lock } from '../../../../icons/app-icons';

/**
 * SecureStorageNotice - A reusable component that displays a message about secure storage
 *
 * @param {Object} props - Component props
 * @param {string} [props.className] - Optional additional CSS classes
 * @param {string} [props.message] - Optional custom message (defaults to keys stored in .env)
 * @returns {JSX.Element} - The secure storage notice component
 */
export function SecureStorageNotice({
  className = '',
  message = 'Keys are stored securely in the keychain',
}) {
  return (
    <div className={`flex items-center mt-3 text-text-muted ${className}`}>
      <Lock className="w-3.5 h-3.5 flex-shrink-0" />
      <span className="text-xs ml-1.5">{message}</span>
    </div>
  );
}
