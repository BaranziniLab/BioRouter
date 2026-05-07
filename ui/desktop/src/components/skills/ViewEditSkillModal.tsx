import { Skill } from './skillUtils';
interface Props { skill: Skill; onClose: () => void; onSaved: () => void; }
export default function ViewEditSkillModal({ onClose }: Props) {
  return <div onClick={onClose} />;
}
