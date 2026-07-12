import { useState, useEffect, useCallback } from 'react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Button } from '../ui/button';
import { Switch } from '../ui/switch';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { Plus, Upload, Globe, Trash2 } from '../icons/app-icons';
import {
  Skill,
  SkillBundle,
  BIOROUTER_SKILLS_DIR,
  OTHER_SKILL_DIRS,
  loadSkillsFromDirs,
  isBuiltinSkill,
} from './skillUtils';
import SkillItem from './SkillItem';
import AddSkillModal from './AddSkillModal';
import CustomSkillModal from './CustomSkillModal';
import BrowseSkillsModal from '../baam/BrowseSkillsModal';
import { toastSuccess, toastError } from '../../toasts';
import { SearchView } from '../conversation/SearchView';
import { getSearchShortcutText } from '../../utils/keyboardShortcuts';
import { ReadableContent } from '../Layout/ReadableContent';
import {
  loadSkillOverrides,
  saveSkillOverrides,
  setSkillOverride,
  isSkillEnabled,
} from '../../store/skillOverrides';

export default function SkillsView() {
  const [bioRouterSkills, setBioRouterSkills] = useState<Skill[]>([]);
  const [otherSkills, setOtherSkills] = useState<Skill[]>([]);
  const [bioBundles, setBioBundles] = useState<SkillBundle[]>([]);
  const [otherBundles, setOtherBundles] = useState<SkillBundle[]>([]);
  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const [isCustomModalOpen, setIsCustomModalOpen] = useState(false);
  const [isBrowseModalOpen, setIsBrowseModalOpen] = useState(false);
  const [skillToDelete, setSkillToDelete] = useState<Skill | null>(null);
  const [bundleToDelete, setBundleToDelete] = useState<SkillBundle | null>(null);
  const [isDeletingSkill, setIsDeletingSkill] = useState(false);
  const [isDeletingBundle, setIsDeletingBundle] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [overrideTrigger, setOverrideTrigger] = useState(0);

  const loadSkills = useCallback(async () => {
    try {
      const [brResult, otherResult] = await Promise.all([
        loadSkillsFromDirs([BIOROUTER_SKILLS_DIR]),
        loadSkillsFromDirs(OTHER_SKILL_DIRS),
      ]);
      setBioRouterSkills(brResult.singles);
      setBioBundles(brResult.bundles);
      setOtherSkills(otherResult.singles);
      setOtherBundles(otherResult.bundles);
    } catch {
      setBioRouterSkills([]);
      setBioBundles([]);
      setOtherSkills([]);
      setOtherBundles([]);
    }
  }, []);

  useEffect(() => {
    loadSkills();
    loadSkillOverrides();
  }, [loadSkills]);

  const handleToggle = async (skill: Skill, enabled: boolean) => {
    const previous = isSkillEnabled(skill.name);
    setSkillOverride(skill.name, enabled);
    setOverrideTrigger((prev) => prev + 1);
    try {
      await saveSkillOverrides();
    } catch (error) {
      setSkillOverride(skill.name, previous);
      setOverrideTrigger((prev) => prev + 1);
      toastError({
        title: skill.name,
        msg: error instanceof Error ? error.message : 'Could not save the skill preference',
      });
    }
  };

  const handleBundleToggle = async (bundle: SkillBundle, enabled: boolean) => {
    const previous = isSkillEnabled(bundle.bundleName);
    setSkillOverride(bundle.bundleName, enabled);
    setOverrideTrigger((prev) => prev + 1);
    try {
      await saveSkillOverrides();
    } catch (error) {
      setSkillOverride(bundle.bundleName, previous);
      setOverrideTrigger((prev) => prev + 1);
      toastError({
        title: bundle.bundleName,
        msg: error instanceof Error ? error.message : 'Could not save the bundle preference',
      });
    }
  };

  const filterSkill = (skill: Skill) => {
    if (!searchTerm) return true;
    const q = searchTerm.toLowerCase();
    return skill.name.toLowerCase().includes(q) || skill.description.toLowerCase().includes(q);
  };

  const filterBundle = (bundle: SkillBundle) => {
    if (!searchTerm) return true;
    const q = searchTerm.toLowerCase();
    return (
      bundle.bundleName.toLowerCase().includes(q) ||
      bundle.skills.some(
        (s) => s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q)
      )
    );
  };

  const handleOpen = async (skill: Skill) => {
    await window.electron.openDirectoryInExplorer(skill.folderPath);
  };

  const handleOpenBundle = async (bundle: SkillBundle) => {
    await window.electron.openDirectoryInExplorer(bundle.folderPath);
  };

  const confirmDeleteSkill = async () => {
    if (!skillToDelete) return;
    if (isBuiltinSkill(skillToDelete.name)) {
      toastError({ title: skillToDelete.name, msg: 'Built-in skills cannot be deleted' });
      setSkillToDelete(null);
      return;
    }
    setIsDeletingSkill(true);
    const skill = skillToDelete;
    const ok = await window.electron.deleteDirectory(skill.folderPath);
    setIsDeletingSkill(false);
    setSkillToDelete(null);
    if (ok) {
      setBioRouterSkills((prev) => prev.filter((s) => s.folderPath !== skill.folderPath));
      setOtherSkills((prev) => prev.filter((s) => s.folderPath !== skill.folderPath));
      toastSuccess({ title: skill.name, msg: 'Skill deleted' });
    } else {
      toastError({ title: 'Delete failed', msg: `Could not delete ${skill.folderPath}` });
    }
  };

  const confirmDeleteBundle = async () => {
    if (!bundleToDelete) return;
    setIsDeletingBundle(true);
    const bundle = bundleToDelete;
    const ok = await window.electron.deleteDirectory(bundle.folderPath);
    setIsDeletingBundle(false);
    setBundleToDelete(null);
    if (ok) {
      setBioBundles((prev) => prev.filter((b) => b.folderPath !== bundle.folderPath));
      setOtherBundles((prev) => prev.filter((b) => b.folderPath !== bundle.folderPath));
      toastSuccess({ title: bundle.bundleName, msg: 'Bundle deleted' });
    } else {
      toastError({ title: 'Delete failed', msg: `Could not delete ${bundle.folderPath}` });
    }
  };

  const handleShare = async (skill: Skill) => {
    try {
      await navigator.clipboard.writeText(skill.content);
      toastSuccess({ title: skill.name, msg: 'SKILL.md copied to clipboard' });
    } catch {
      toastError({ title: 'Copy failed', msg: 'Could not copy to clipboard' });
    }
  };

  const filteredBR = bioRouterSkills.filter(filterSkill);
  const filteredOther = otherSkills.filter(filterSkill);
  const filteredBRBundles = bioBundles.filter(filterBundle);
  const filteredOtherBundles = otherBundles.filter(filterBundle);

  const totalBR = filteredBR.length + filteredBRBundles.length;
  const totalOther = filteredOther.length + filteredOtherBundles.length;

  return (
    <MainPanelLayout>
      <div
        className="flex flex-col min-w-0 flex-1 overflow-y-auto relative"
        data-search-scroll-area
      >
        {/* Header */}
        <ReadableContent className="px-8 pt-12 pb-6 border-b border-border-subtle flex-shrink-0">
          <div className="flex flex-col page-transition">
            <h1 className="text-2xl font-semibold tracking-tight mb-1">Skills</h1>
            <p className="text-sm text-text-muted mb-0">
              Reusable instruction sets that guide BioRouter's behavior. {getSearchShortcutText()}{' '}
              to search.
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
              onClick={() => setIsBrowseModalOpen(true)}
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
        </ReadableContent>

        {/* List */}
        <SearchView
          onSearch={(term, _caseSensitive) => setSearchTerm(term)}
          placeholder="Search skills..."
        >
          <ReadableContent key={overrideTrigger} className="px-8 py-4">
            {totalBR > 0 && (
              <>
                <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3 flex items-center gap-2">
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-background-info flex-shrink-0" />
                  BioRouter Skills ({totalBR})
                </h2>
                <div className="biorouter-list-shell">
                  {filteredBRBundles.map((bundle) => (
                    <BundleItem
                      key={bundle.folderPath}
                      bundle={bundle}
                      enabled={isSkillEnabled(bundle.bundleName)}
                      onClick={() => handleOpenBundle(bundle)}
                      onDelete={() => setBundleToDelete(bundle)}
                      onToggle={(e) => handleBundleToggle(bundle, e)}
                    />
                  ))}
                  {filteredBR.map((skill) => (
                    <SkillItem
                      key={skill.folderPath}
                      skill={skill}
                      enabled={isSkillEnabled(skill.name)}
                      onClick={() => handleOpen(skill)}
                      onDelete={() => setSkillToDelete(skill)}
                      onShare={() => handleShare(skill)}
                      onToggle={(e) => handleToggle(skill, e)}
                    />
                  ))}
                </div>
              </>
            )}

            {totalOther > 0 && (
              <>
                <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider mt-6 mb-3 flex items-center gap-2">
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-background-strong flex-shrink-0" />
                  Skills From Other Agents ({totalOther})
                </h2>
                <div className="biorouter-list-shell">
                  {filteredOtherBundles.map((bundle) => (
                    <BundleItem
                      key={bundle.folderPath}
                      bundle={bundle}
                      enabled={isSkillEnabled(bundle.bundleName)}
                      onClick={() => handleOpenBundle(bundle)}
                      onDelete={() => setBundleToDelete(bundle)}
                      onToggle={(e) => handleBundleToggle(bundle, e)}
                    />
                  ))}
                  {filteredOther.map((skill) => (
                    <SkillItem
                      key={skill.folderPath}
                      skill={skill}
                      enabled={isSkillEnabled(skill.name)}
                      onClick={() => handleOpen(skill)}
                      onDelete={() => setSkillToDelete(skill)}
                      onShare={() => handleShare(skill)}
                      onToggle={(e) => handleToggle(skill, e)}
                    />
                  ))}
                </div>
              </>
            )}

            {totalBR === 0 && totalOther === 0 && (
              <p className="text-sm text-text-muted mt-10 text-center">
                {searchTerm
                  ? 'No skills match your search.'
                  : 'No skills found. Add one to get started.'}
              </p>
            )}
          </ReadableContent>
        </SearchView>
      </div>

      {isAddModalOpen && (
        <AddSkillModal onClose={() => setIsAddModalOpen(false)} onSaved={loadSkills} />
      )}
      {isCustomModalOpen && (
        <CustomSkillModal onClose={() => setIsCustomModalOpen(false)} onSaved={loadSkills} />
      )}
      {isBrowseModalOpen && (
        <BrowseSkillsModal
          onClose={() => setIsBrowseModalOpen(false)}
          onInstalled={loadSkills}
          installedIds={
            new Set(
              [...bioRouterSkills, ...otherSkills]
                .flatMap((s) => [
                  s.name.toLowerCase(),
                  s.folderPath.split('/').pop()?.toLowerCase() ?? '',
                ])
                .concat(
                  [...bioBundles, ...otherBundles].flatMap((b) => [
                    b.bundleName.toLowerCase(),
                    b.folderPath.split('/').pop()?.toLowerCase() ?? '',
                  ])
                )
                .filter(Boolean)
            )
          }
        />
      )}

      <ConfirmationModal
        isOpen={skillToDelete !== null}
        title={`Delete "${skillToDelete?.name}"?`}
        message="This will permanently remove the skill folder from disk. This action cannot be undone."
        confirmLabel="Delete"
        cancelLabel="Cancel"
        confirmVariant="destructive"
        isSubmitting={isDeletingSkill}
        onConfirm={confirmDeleteSkill}
        onCancel={() => setSkillToDelete(null)}
      />

      <ConfirmationModal
        isOpen={bundleToDelete !== null}
        title={`Delete bundle "${bundleToDelete?.bundleName}"?`}
        message={`This will permanently remove all ${bundleToDelete?.skills.length ?? 0} skills in this bundle. This action cannot be undone.`}
        confirmLabel="Delete Bundle"
        cancelLabel="Cancel"
        confirmVariant="destructive"
        isSubmitting={isDeletingBundle}
        onConfirm={confirmDeleteBundle}
        onCancel={() => setBundleToDelete(null)}
      />
    </MainPanelLayout>
  );
}

// ---------------------------------------------------------------------------
// Inline bundle row component
// ---------------------------------------------------------------------------
interface BundleItemProps {
  bundle: SkillBundle;
  enabled: boolean;
  onClick: () => void;
  onDelete: () => void;
  onToggle: (enabled: boolean) => void;
}

function BundleItem({ bundle, enabled, onClick, onDelete, onToggle }: BundleItemProps) {
  return (
    <div className="biorouter-list-row flex items-start gap-3 px-3 py-3 group">
      <button
        type="button"
        className="flex-1 min-w-0 cursor-pointer rounded-sm text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-border-focus"
        onClick={onClick}
        aria-label={`Open skill bundle ${bundle.bundleName}`}
      >
        <div className="flex items-center gap-1.5">
          <p className="text-sm text-text-default">{bundle.bundleName}</p>
          <span className="text-[11px] text-text-subtle">· {bundle.skills.length} skills</span>
        </div>
        <p className="text-xs text-text-subtle mt-1 font-mono leading-relaxed">
          {bundle.skills.map((s) => s.name).join(' · ')}
        </p>
      </button>
      <div className="flex items-center gap-2 flex-shrink-0 mt-0.5">
        <div
          className="flex gap-1 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100"
          onClick={(e) => e.stopPropagation()}
        >
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0 text-text-danger hover:bg-background-danger/10"
            onClick={onDelete}
            title="Delete bundle"
            aria-label={`Delete skill bundle ${bundle.bundleName}`}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
        <div onClick={(e) => e.stopPropagation()}>
          <Switch
            checked={enabled}
            onCheckedChange={onToggle}
            variant="mono"
            aria-label={`${enabled ? 'Disable' : 'Enable'} ${bundle.bundleName}`}
          />
        </div>
      </div>
    </div>
  );
}
