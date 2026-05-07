import { useState, useEffect, useCallback } from 'react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Button } from '../ui/button';
import { Plus } from '../icons/app-icons';
import { Upload, Globe } from 'lucide-react';
import { Skill, BIOROUTER_SKILLS_DIR, OTHER_SKILL_DIRS, loadSkillsFromDirs } from './skillUtils';
import SkillItem from './SkillItem';
import AddSkillModal from './AddSkillModal';
import CustomSkillModal from './CustomSkillModal';
import ViewEditSkillModal from './ViewEditSkillModal';
import { toastSuccess, toastError } from '../../toasts';
import { SearchView } from '../conversation/SearchView';
import { getSearchShortcutText } from '../../utils/keyboardShortcuts';

export default function SkillsView() {
  const [bioRouterSkills, setBioRouterSkills] = useState<Skill[]>([]);
  const [otherSkills, setOtherSkills] = useState<Skill[]>([]);
  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const [isCustomModalOpen, setIsCustomModalOpen] = useState(false);
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [searchTerm, setSearchTerm] = useState('');

  const loadSkills = useCallback(async () => {
    try {
      const [brSkills, others] = await Promise.all([
        loadSkillsFromDirs([BIOROUTER_SKILLS_DIR]),
        loadSkillsFromDirs(OTHER_SKILL_DIRS),
      ]);
      setBioRouterSkills(brSkills);
      setOtherSkills(others);
    } catch {
      setBioRouterSkills([]);
      setOtherSkills([]);
    }
  }, []);

  useEffect(() => { loadSkills(); }, [loadSkills]);

  const filterSkill = (skill: Skill) => {
    if (!searchTerm) return true;
    const q = searchTerm.toLowerCase();
    return skill.name.toLowerCase().includes(q) || skill.description.toLowerCase().includes(q);
  };

  const handleDelete = async (skill: Skill) => {
    const result = await window.electron.showMessageBox({
      type: 'warning',
      buttons: ['Cancel', 'Delete'],
      defaultId: 0,
      title: 'Delete Skill',
      message: `Delete "${skill.name}"?`,
      detail: 'This will remove the file from disk. This action cannot be undone.',
    });
    if (result.response !== 1) return;
    const ok = await window.electron.deleteFile(skill.filePath);
    if (ok) {
      toastSuccess({ title: skill.name, msg: 'Skill deleted' });
      loadSkills();
    } else {
      toastError({ title: 'Delete failed', msg: `Could not delete ${skill.filePath}` });
    }
  };

  const handleShare = async (skill: Skill) => {
    try {
      await navigator.clipboard.writeText(skill.content);
      toastSuccess({ title: skill.name, msg: 'Copied to clipboard as Markdown' });
    } catch {
      toastError({ title: 'Copy failed', msg: 'Could not copy to clipboard' });
    }
  };

  const filteredBR = bioRouterSkills.filter(filterSkill);
  const filteredOther = otherSkills.filter(filterSkill);

  return (
    <MainPanelLayout>
      <div className="flex flex-col min-w-0 flex-1 overflow-y-auto relative" data-search-scroll-area>
        {/* Header */}
        <div className="px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
          <div className="flex flex-col page-transition">
            <h1 className="text-2xl font-semibold tracking-tight mb-1">Skills</h1>
            <p className="text-sm text-text-muted mb-0">
              Reusable instruction sets that guide BioRouter's behavior.{' '}
              {getSearchShortcutText()} to search.
            </p>
          </div>
          <div className="flex gap-3 mt-5">
            <Button
              className="flex items-center gap-2"
              variant="default"
              onClick={() => setIsAddModalOpen(true)}
            >
              <Upload className="h-4 w-4" />
              Add Skill
            </Button>
            <Button
              className="flex items-center gap-2"
              variant="outline"
              onClick={() =>
                window.open(
                  'https://baranzinilab.github.io/biorouter-landing/baam.html',
                  '_blank'
                )
              }
            >
              <Globe className="h-4 w-4" />
              Browse Skills
            </Button>
            <Button
              className="flex items-center gap-2"
              variant="outline"
              onClick={() => setIsCustomModalOpen(true)}
            >
              <Plus className="h-4 w-4" />
              Add Custom Skill
            </Button>
          </div>
        </div>

        {/* List */}
        <SearchView onSearch={(term, _caseSensitive) => setSearchTerm(term)} placeholder="Search skills...">
          <div className="px-6 py-4">
            {filteredBR.length > 0 && (
              <>
                <p className="text-[11px] font-medium text-text-subtle uppercase tracking-widest mb-2 px-2 flex items-center gap-1.5">
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-blue-500" />
                  BioRouter Skills ({filteredBR.length})
                </p>
                {filteredBR.map((skill) => (
                  <SkillItem
                    key={skill.filePath}
                    skill={skill}
                    onClick={() => setSelectedSkill(skill)}
                    onDelete={() => handleDelete(skill)}
                    onShare={() => handleShare(skill)}
                  />
                ))}
              </>
            )}

            {filteredOther.length > 0 && (
              <>
                <p className="text-[11px] font-medium text-text-subtle uppercase tracking-widest mt-6 mb-2 px-2 flex items-center gap-1.5">
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-neutral-400" />
                  Skills From Other Agents ({filteredOther.length})
                </p>
                {filteredOther.map((skill) => (
                  <SkillItem
                    key={skill.filePath}
                    skill={skill}
                    onClick={() => setSelectedSkill(skill)}
                    onDelete={() => handleDelete(skill)}
                    onShare={() => handleShare(skill)}
                  />
                ))}
              </>
            )}

            {filteredBR.length === 0 && filteredOther.length === 0 && (
              <p className="text-sm text-text-muted mt-10 text-center">
                {searchTerm
                  ? 'No skills match your search.'
                  : 'No skills found. Add one to get started.'}
              </p>
            )}
          </div>
        </SearchView>
      </div>

      {isAddModalOpen && (
        <AddSkillModal onClose={() => setIsAddModalOpen(false)} onSaved={loadSkills} />
      )}
      {isCustomModalOpen && (
        <CustomSkillModal onClose={() => setIsCustomModalOpen(false)} onSaved={loadSkills} />
      )}
      {selectedSkill && (
        <ViewEditSkillModal
          skill={selectedSkill}
          onClose={() => setSelectedSkill(null)}
          onSaved={loadSkills}
        />
      )}
    </MainPanelLayout>
  );
}
