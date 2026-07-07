import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react';
import { createPortal } from 'react-dom';
import { ItemIcon } from './ItemIcon';
import BuiltInBadge from './ui/BuiltInBadge';
import { CommandType, getActive, getSessionExtensions, getSlashCommands, listBases } from '../api';
import { getInitialWorkingDir } from '../utils/workingDir';
import { useConfig } from './ConfigContext';
import { ALL_SKILL_DIRS, loadSkillsFromDirs } from './skills/skillUtils';
import bundledExtensionsData from './settings/extensions/bundled-extensions.json';

type DisplayItemType = CommandType | 'Directory' | 'File' | 'KnowledgeBase' | 'Skill' | 'Extension';

const typeOrder: Record<DisplayItemType, number> = {
  Builtin: 0,
  Workflow: 1,
  KnowledgeBase: 2,
  Skill: 3,
  Extension: 4,
  Directory: 5,
  File: 6,
};

// Slash commands that are purely a UI convenience. Keyed by command name (no
// leading '/') and inserted as visible routed resource markers.
const CLIENT_INSERT_COMMANDS: Record<string, { description: string; insert: string }> = {
  knowledge: {
    description: 'Use the Knowledge extension',
    insert: '/ext:knowledge ',
  },
  diverge: {
    description: 'Branch this conversation into a new chat, keeping the full history — press Enter',
    insert: '/diverge',
  },
};

const REMOVED_SLASH_COMMANDS = new Set(['prompt', 'prompts']);
const COMPACT_EXTENSION_ALIASES: Record<string, { canonical: string; name: string; label: string }> = {
  agentdrafter: { canonical: 'agent_drafter', name: 'agentdrafter', label: 'Agent Drafter' },
  autovisualiser: {
    canonical: 'autovisualiser',
    name: 'autovisualizer',
    label: 'Auto Visualiser',
  },
  extensionmanager: {
    canonical: 'Extension Manager',
    name: 'extensionmanager',
    label: 'Extension Manager',
  },
};

const compactExtensionAliasFor = (name: string) =>
  COMPACT_EXTENSION_ALIASES[name.replace(/[\s_-]+/g, '').toLowerCase()];

const normalizedExtensionName = (name: string) => name.replace(/[\s_-]+/g, '').toLowerCase();

const isKnownBuiltInExtension = (name: string) => {
  const normalized = normalizedExtensionName(name);
  return (
    Boolean(COMPACT_EXTENSION_ALIASES[normalized]) ||
    bundledExtensionsData.some((extension) => normalizedExtensionName(extension.name) === normalized) ||
    ['todo', 'skills', 'extensionmanager', 'chatrecall', 'codeexecution'].includes(normalized)
  );
};

const referenceInsert = (item: DisplayItem) => {
  switch (item.itemType) {
    case 'KnowledgeBase':
      return `/kb:${item.relativePath} `;
    case 'Skill':
      return `/skill:${item.relativePath} `;
    case 'Extension':
      return `/ext:${item.relativePath} `;
    default:
      return null;
  }
};

export const getMentionInsertText = (item: DisplayItem) => {
  const clientInsert = CLIENT_INSERT_COMMANDS[item.name];
  if (clientInsert) return clientInsert.insert;

  const insertedReference = referenceInsert(item);
  if (insertedReference) return insertedReference;

  return ['Builtin', 'Workflow'].includes(item.itemType) ? `/${item.name}` : item.extra;
};

export interface DisplayItem {
  name: string;
  extra: string;
  itemType: DisplayItemType;
  relativePath: string;
  builtIn?: boolean;
}

export interface DisplayItemWithMatch extends DisplayItem {
  matchScore: number;
  matches: number[];
  matchedText: string;
}

const uniqueDisplayItems = (displayItems: DisplayItem[]) => {
  const seen = new Set<string>();
  return displayItems.filter((item) => {
    const key = `${item.itemType}\0${item.relativePath}\0${item.name}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
};

const MAX_FILE_SCAN_RESULTS = 300;
const MAX_FILE_DISPLAY_RESULTS = 80;

interface MentionPopoverProps {
  isOpen: boolean;
  onClose: () => void;
  onSelect: (filePath: string) => void;
  position: { x: number; y: number };
  query: string;
  isSlashCommand: boolean;
  selectedIndex: number;
  onSelectedIndexChange: (index: number) => void;
  workingDir?: string;
  sessionId?: string | null;
}

// Enhanced fuzzy matching algorithm
const fuzzyMatch = (pattern: string, text: string): { score: number; matches: number[] } => {
  if (!pattern) return { score: 0, matches: [] };

  const patternLower = pattern.toLowerCase();
  const textLower = text.toLowerCase();
  const matches: number[] = [];

  let patternIndex = 0;
  let score = 0;
  let consecutiveMatches = 0;

  for (let i = 0; i < textLower.length && patternIndex < patternLower.length; i++) {
    if (textLower[i] === patternLower[patternIndex]) {
      matches.push(i);
      patternIndex++;
      consecutiveMatches++;

      // Bonus for consecutive matches
      score += consecutiveMatches * 3;

      // Bonus for matches at word boundaries or path separators
      if (
        i === 0 ||
        textLower[i - 1] === '/' ||
        textLower[i - 1] === '_' ||
        textLower[i - 1] === '-' ||
        textLower[i - 1] === '.'
      ) {
        score += 10;
      }

      // Bonus for matching the start of the filename (after last /)
      const lastSlash = textLower.lastIndexOf('/', i);
      if (lastSlash !== -1 && i === lastSlash + 1) {
        score += 15;
      }
    } else {
      consecutiveMatches = 0;
    }
  }

  // Only return a score if all pattern characters were matched
  if (patternIndex === patternLower.length) {
    // Less penalty for longer strings to allow nested files to rank well
    score -= text.length * 0.05;

    // Bonus for exact substring matches
    if (textLower.includes(patternLower)) {
      score += 20;
    }

    // Bonus for matching the filename specifically (not just the path)
    const fileName = text.split('/').pop()?.toLowerCase() || '';
    if (fileName.includes(patternLower)) {
      score += 25;
    }

    return { score, matches };
  }

  return { score: -1, matches: [] };
};

const MentionPopover = forwardRef<
  { getDisplayFiles: () => DisplayItemWithMatch[]; selectFile: (index: number) => void },
  MentionPopoverProps
>(
  (
    {
      isOpen,
      onClose,
      onSelect,
      position,
      query,
      isSlashCommand,
      selectedIndex,
      onSelectedIndexChange,
      workingDir,
      sessionId,
    },
    ref
  ) => {
    const [items, setItems] = useState<DisplayItem[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const popoverRef = useRef<HTMLDivElement>(null);
    const listRef = useRef<HTMLDivElement>(null);
    const currentWorkingDir = workingDir ?? getInitialWorkingDir();
    const { extensionsList } = useConfig();

    const scanDirectoryFromRoot = useCallback(
      async (
        dirPath: string,
        relativePath = '',
        depth = 0,
        limitRef = { count: 0 }
      ): Promise<DisplayItem[]> => {
        if (depth > 1 || limitRef.count >= MAX_FILE_SCAN_RESULTS) return [];

        try {
          const items = await window.electron.listFiles(dirPath);
          const results: DisplayItem[] = [];

          // Common directories to prioritize or skip
          const priorityDirs = [
            'Desktop',
            'Documents',
            'Downloads',
            'Projects',
            'Development',
            'Code',
            'src',
            'components',
            'icons',
          ];
          const skipDirs = [
            '.git',
            '.svn',
            '.hg',
            'node_modules',
            '__pycache__',
            'target',
            'dist',
            'build',
            '.cache',
            '.npm',
            '.yarn',
            'Library',
            'System',
            'Applications',
            '.Trash',
          ];

          const allowedHiddenDirs = [
            '.github',
            '.vscode',
            '.idea',
            '.config',
            '.gitlab',
            '.circleci',
            '.azure',
            '.jenkins',
          ];

          const skipDirsAtDepth =
            depth > 1 ? ['.git', '.svn', '.hg', 'node_modules', '__pycache__'] : skipDirs;

          // Sort items to prioritize certain directories
          const sortedItems = items.sort((a, b) => {
            const aPriority = priorityDirs.includes(a);
            const bPriority = priorityDirs.includes(b);
            if (aPriority && !bPriority) return -1;
            if (!aPriority && bPriority) return 1;
            return a.localeCompare(b);
          });

          const itemLimit = depth === 0 ? 40 : 25;
          const queryForRecursion = query.trim();

          for (const item of sortedItems.slice(0, itemLimit)) {
            if (limitRef.count >= MAX_FILE_SCAN_RESULTS) break;
            const fullPath = `${dirPath}/${item}`;
            const itemRelativePath = relativePath ? `${relativePath}/${item}` : item;

            // Skip items in the skip list
            if (skipDirsAtDepth.includes(item)) {
              continue;
            }

            // Skip hidden items except for allowed hidden directories
            if (item.startsWith('.') && !allowedHiddenDirs.includes(item)) {
              continue;
            }

            // First, check if this looks like a file based on extension
            const hasExtension = item.includes('.');
            const ext = item.split('.').pop()?.toLowerCase();
            const commonExtensions = [
              // Code items
              'txt',
              'md',
              'js',
              'ts',
              'jsx',
              'tsx',
              'py',
              'java',
              'cpp',
              'c',
              'h',
              'css',
              'html',
              'json',
              'xml',
              'yaml',
              'yml',
              'toml',
              'ini',
              'cfg',
              'sh',
              'bat',
              'ps1',
              'rb',
              'go',
              'rs',
              'php',
              'sql',
              'r',
              'scala',
              'swift',
              'kt',
              'dart',
              'vue',
              'svelte',
              'astro',
              'scss',
              'less',
              // Documentation
              'readme',
              'license',
              'changelog',
              'contributing',
              // Config items
              'gitignore',
              'dockerignore',
              'editorconfig',
              'prettierrc',
              'eslintrc',
              // Images and assets
              'png',
              'jpg',
              'jpeg',
              'gif',
              'svg',
              'ico',
              'webp',
              'bmp',
              'tiff',
              'tif',
              // Vector and design items
              'ai',
              'eps',
              'sketch',
              'fig',
              'xd',
              'psd',
              // Other common items
              'pdf',
              'doc',
              'docx',
              'xls',
              'xlsx',
              'ppt',
              'pptx',
            ];

            // If it has a known file extension, treat it as a file
            if (hasExtension && ext && commonExtensions.includes(ext)) {
              results.push({
                extra: fullPath,
                name: item,
                itemType: 'File',
                relativePath: itemRelativePath,
              });
              limitRef.count += 1;
              continue;
            }

            // If it's a known file without extension (README, LICENSE, etc.)
            const knownFiles = [
              'readme',
              'license',
              'changelog',
              'contributing',
              'dockerfile',
              'makefile',
            ];
            if (!hasExtension && knownFiles.includes(item.toLowerCase())) {
              results.push({
                extra: fullPath,
                name: item,
                itemType: 'File',
                relativePath: itemRelativePath,
              });
              limitRef.count += 1;
              continue;
            }

            // Otherwise, try to determine if it's a directory
            try {
              await window.electron.listFiles(fullPath);

              results.push({
                name: item,
                extra: fullPath,
                itemType: 'Directory',
                relativePath: itemRelativePath,
              });
              limitRef.count += 1;

              const shouldRecurse =
                depth === 0 &&
                (priorityDirs.includes(item) ||
                  (queryForRecursion.length >= 2 &&
                    fuzzyMatch(queryForRecursion, itemRelativePath).score >=
                      Math.max(8, queryForRecursion.length * 4)));

              if (shouldRecurse) {
                const subFiles = await scanDirectoryFromRoot(
                  fullPath,
                  itemRelativePath,
                  depth + 1,
                  limitRef
                );
                results.push(...subFiles);
              }
            } catch {
              // If we can't list it and it doesn't have a known extension, skip it
              // This could be a file with an unknown extension or a permission issue
            }
          }

          return results;
        } catch (error) {
          console.error(`Error scanning directory ${dirPath}:`, error);
          return [];
        }
      },
      [query]
    );

    const scanFilesFromRoot = useCallback(async () => {
      setIsLoading(true);
      try {
        let startPath = currentWorkingDir;

        if (!startPath) {
          if (window.electron.platform === 'win32') {
            startPath = 'C:\\Users';
          } else if (window.electron.platform === 'linux') {
            startPath = '/home';
          } else {
            startPath = '/Users'; // Default to macOS
          }
        }

        const scannedFiles = await scanDirectoryFromRoot(startPath);
        setItems(scannedFiles);
      } catch (error) {
        console.error('Error scanning items from root:', error);
        setItems([]);
      } finally {
        setIsLoading(false);
      }
    }, [scanDirectoryFromRoot, currentWorkingDir]);

    const loadReferenceItems = useCallback(
      async (includeCommands: boolean) => {
        const [
          commandsResponse,
          basesResponse,
          activeResponse,
          skillsResult,
          sessionExtensions,
        ] = await Promise.all([
          includeCommands
            ? getSlashCommands({ throwOnError: true })
            : Promise.resolve({ data: { commands: [] } }),
          listBases({ throwOnError: false }),
          getActive({
            query: sessionId ? { session_id: sessionId } : undefined,
            throwOnError: false,
          }),
          loadSkillsFromDirs(ALL_SKILL_DIRS).catch(() => ({ singles: [], bundles: [] })),
          sessionId
            ? getSessionExtensions({ path: { session_id: sessionId } }).catch(() => null)
            : Promise.resolve(null),
        ]);
        const commandItems: DisplayItem[] = (commandsResponse.data?.commands || [])
          .filter((cmd) => !REMOVED_SLASH_COMMANDS.has(cmd.command.replace(/^\/+/, '').toLowerCase()))
          .map((cmd) => ({
            name: cmd.command,
            extra: cmd.help,
            itemType: cmd.command_type,
            relativePath: cmd.command,
            builtIn: cmd.command_type === 'Builtin',
          }));
        if (includeCommands) {
          const existingNames = new Set(commandItems.map((c) => c.name));
          for (const [name, def] of Object.entries(CLIENT_INSERT_COMMANDS)) {
            if (existingNames.has(name)) continue;
            commandItems.push({
              name,
              extra: def.description,
              itemType: 'Builtin',
              relativePath: name,
              builtIn: true,
            });
          }
        }

        const hiddenKbIds = new Set(activeResponse.data?.hidden_kbs ?? []);
        const activeKbId = activeResponse.data?.active_kb ?? null;
        for (const base of basesResponse.data ?? []) {
          if (hiddenKbIds.has(base.id)) continue;
          commandItems.push({
            name: `kb:${base.name}`,
            extra: `${activeKbId === base.id ? 'Focused knowledge base' : 'Visible knowledge base'} · ${base.id}`,
            itemType: 'KnowledgeBase',
            relativePath: base.id,
          });
        }
        for (const bundle of skillsResult.bundles) {
          commandItems.push({
            name: `skill:${bundle.bundleName}`,
            extra: `${bundle.skills.length} skill${bundle.skills.length === 1 ? '' : 's'} in bundle`,
            itemType: 'Skill',
            relativePath: bundle.bundleName,
          });
        }
        for (const skill of skillsResult.singles) {
          commandItems.push({
            name: `skill:${skill.name}`,
            extra: skill.description,
            itemType: 'Skill',
            relativePath: skill.name,
          });
        }

        const enabledSessionExtensions = new Set(
          sessionExtensions?.data?.extensions?.map((extension) => extension.name) ?? []
        );
        for (const extension of extensionsList) {
          if (compactExtensionAliasFor(extension.name)) continue;
          const enabled = sessionId
            ? enabledSessionExtensions.has(extension.name)
            : extension.enabled;
          if (!enabled) continue;
          commandItems.push({
            name: `ext:${extension.name}`,
            extra: extension.description || 'Enabled extension',
            itemType: 'Extension',
            relativePath: extension.name,
            builtIn: isKnownBuiltInExtension(extension.name),
          });
        }

        for (const extension of bundledExtensionsData) {
          if (compactExtensionAliasFor(extension.name)) continue;
          commandItems.push({
            name: `ext:${extension.name}`,
            extra: extension.description || extension.display_name || extension.name,
            itemType: 'Extension',
            relativePath: extension.name,
            builtIn: true,
          });
        }
        for (const alias of Object.values(COMPACT_EXTENSION_ALIASES)) {
          const bundledExtension = bundledExtensionsData.find(
            (extension) => extension.name === alias.canonical
          );
          commandItems.push({
            name: `ext:${alias.name}`,
            extra: bundledExtension?.description || alias.label,
            itemType: 'Extension',
            relativePath: alias.name,
            builtIn: true,
          });
        }

        return uniqueDisplayItems(commandItems);
      },
      [extensionsList, sessionId]
    );

    const compareByType = (a: DisplayItemWithMatch, b: DisplayItemWithMatch) => {
      const orderA = typeOrder[a.itemType] ?? Number.MAX_SAFE_INTEGER;
      const orderB = typeOrder[b.itemType] ?? Number.MAX_SAFE_INTEGER;
      return orderA - orderB;
    };

    const displayItems = useMemo((): DisplayItemWithMatch[] => {
      if (!query.trim()) {
        return items
          .map((file) => ({
            ...file,
            matchScore: 0,
            matches: [],
            matchedText: file.name,
            depth: currentWorkingDir
              ? file.extra.replace(currentWorkingDir, '').split('/').length - 1
              : 0,
          }))
          .sort((a, b) => {
            if (a.depth !== b.depth) return a.depth - b.depth;
            const typeComparison = compareByType(a, b);
            return typeComparison || a.name.localeCompare(b.name);
          });
      }

      const matchedItems = items
        .map((file) => {
          const matches = [
            { match: fuzzyMatch(query, file.name), text: file.name },
            { match: fuzzyMatch(query, file.relativePath), text: file.relativePath },
          ];
          if (isSlashCommand) {
            matches.push({ match: fuzzyMatch(query, file.extra), text: file.extra });
          }

          const { match: bestMatch, text: matchedText } = matches.reduce((best, current) =>
            current.match.score > best.match.score ? current : best
          );

          let finalScore = bestMatch.score;
          if (finalScore > 0 && currentWorkingDir) {
            const depth = file.extra.replace(currentWorkingDir, '').split('/').length - 1;
            finalScore += depth <= 1 ? 50 : depth <= 2 ? 30 : depth <= 3 ? 15 : 0;
          }

          return {
            ...file,
            matchScore: finalScore,
            matches: bestMatch.matches,
            matchedText,
          };
        })
        .filter((file) => file.matchScore > 0)
        .sort((a, b) => {
          // Sort by score first, then prefer items over directories, then alphabetically
          const scoreDiff = b.matchScore - a.matchScore;
          if (Math.abs(scoreDiff) >= 1) return scoreDiff;
          const typeComparison = compareByType(a, b);
          return typeComparison || a.name.localeCompare(b.name);
        });

      return isSlashCommand ? matchedItems : matchedItems.slice(0, MAX_FILE_DISPLAY_RESULTS);
    }, [items, query, currentWorkingDir, isSlashCommand]);

    // Expose methods to parent component
    useImperativeHandle(
      ref,
      () => ({
        getDisplayFiles: () => displayItems,
        selectFile: (index: number) => {
          const item = displayItems[index];
          if (!item) return;
          onSelect(getMentionInsertText(item));
          onClose();
        },
      }),
      [displayItems, onSelect, onClose]
    );

    useEffect(() => {
      let cancelled = false;
      let timeoutId: ReturnType<typeof setTimeout> | undefined;

      const loadData = async () => {
        if (isSlashCommand) {
          setIsLoading(true);
          try {
            const commandItems = await loadReferenceItems(true);
            if (!cancelled) setItems(commandItems);
          } catch (error) {
            console.error('Error loading slash commands:', error);
            if (!cancelled) setItems([]);
          } finally {
            if (!cancelled) setIsLoading(false);
          }
          return;
        }

        if (query.trim().length < 2) {
          setItems([]);
          return;
        }

        timeoutId = setTimeout(() => {
          if (!cancelled) void scanFilesFromRoot();
        }, 120);
      };

      if (isOpen) {
        loadData();
      }

      return () => {
        cancelled = true;
        if (timeoutId) clearTimeout(timeoutId);
      };
    }, [isOpen, isSlashCommand, loadReferenceItems, query, scanFilesFromRoot]);

    useEffect(() => {
      const handleClickOutside = (event: MouseEvent) => {
        if (popoverRef.current && !popoverRef.current.contains(event.target as Node)) {
          onClose();
        }
      };

      if (isOpen) {
        document.addEventListener('mousedown', handleClickOutside);
      }

      return () => {
        document.removeEventListener('mousedown', handleClickOutside);
      };
    }, [isOpen, onClose]);

    // Scroll selected item into view
    useEffect(() => {
      if (listRef.current && selectedIndex >= 0 && selectedIndex < displayItems.length) {
        const selectedElement = listRef.current.children[selectedIndex] as HTMLElement;
        if (selectedElement) {
          selectedElement.scrollIntoView({
            block: 'nearest',
            behavior: 'smooth',
          });
        }
      }
    }, [selectedIndex, displayItems.length]);

    const handleItemClick = (index: number) => {
      if (index >= 0 && index < displayItems.length) {
        onSelectedIndexChange(index);
        const displayItem = displayItems[index];
        onSelect(getMentionInsertText(displayItem));
        onClose();
      }
    };

    if (!isOpen) return null;

    const menuWidth = Math.max(320, Math.min(448, window.innerWidth - 16));
    const menuLeft = Math.min(
      Math.max(8, position.x),
      Math.max(8, window.innerWidth - menuWidth - 8)
    );

    const menu = (
      <div
        ref={popoverRef}
        className="biorouter-popover-surface fixed z-50 bg-background-default rounded-xl max-h-72 overflow-hidden"
        style={{
          left: menuLeft,
          top: position.y - 8,
          width: menuWidth,
          transform: 'translateY(-100%)', // Move it fully above
        }}
      >
        <div className="p-1.5 flex flex-col max-h-72">
          {isLoading ? (
            <div className="flex items-center justify-center py-3">
              <div className="animate-spin rounded-full h-4 w-4 border-t-2 border-b-2 border-text-muted"></div>
              <span className="ml-2 text-sm text-text-muted">Scanning files...</span>
            </div>
          ) : (
            <>
              {displayItems.length > 0 && (
                <div className="text-[11px] leading-3 text-text-muted mb-1 px-1">
                  {displayItems.length} item{displayItems.length !== 1 ? 's' : ''} found
                </div>
              )}
              <div
                ref={listRef}
                className="overflow-y-auto flex-1 scrollbar-thin scrollbar-thumb-borderStandard scrollbar-track-transparent"
                style={{ maxHeight: '244px' }}
              >
                {displayItems.map((item, index) => (
                  <div
                    key={`${item.itemType}-${item.relativePath}-${item.name}`}
                    onClick={() => handleItemClick(index)}
                    className={`flex items-center gap-1.5 px-1.5 py-1 rounded ring-1 ring-inset cursor-pointer transition-colors ${
                      index === selectedIndex
                        ? 'ring-border-subtle bg-background-strong/70'
                        : 'ring-transparent hover:ring-border-subtle hover:bg-background-medium'
                    }`}
                  >
                    <div className="flex-shrink-0 text-text-muted">
                      <ItemIcon item={item} />
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-1.5">
                        <div className="text-xs leading-4 truncate text-text-default">
                          {item.name}
                        </div>
                        {item.builtIn && (
                          <BuiltInBadge title="Built in. Ships with Biorouter." />
                        )}
                      </div>
                      <div className="text-[11px] leading-3 truncate text-text-muted">
                        {item.extra}
                      </div>
                    </div>
                  </div>
                ))}

                {!isLoading && displayItems.length === 0 && query && (
                  <div className="p-4 text-center text-text-muted text-sm">
                    No items found matching "{query}"
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      </div>
    );

    return createPortal(menu, document.body);
  }
);

MentionPopover.displayName = 'MentionPopover';

export default MentionPopover;
