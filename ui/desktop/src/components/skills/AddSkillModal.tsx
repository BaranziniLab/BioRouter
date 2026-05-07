interface Props { onClose: () => void; onSaved: () => void; }
export default function AddSkillModal({ onClose }: Props) {
  return <div onClick={onClose} />;
}
