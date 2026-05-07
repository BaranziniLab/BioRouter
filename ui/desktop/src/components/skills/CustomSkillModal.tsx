interface Props { onClose: () => void; onSaved: () => void; }
export default function CustomSkillModal({ onClose }: Props) {
  return <div onClick={onClose} />;
}
