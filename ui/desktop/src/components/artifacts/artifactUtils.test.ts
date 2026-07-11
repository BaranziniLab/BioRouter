import { describe, expect, it } from 'vitest';
import {
  baseToolName,
  fileArtifactPathsFromToolCall,
  languageFromPath,
  languageLabel,
  parseDelimitedTable,
  resolveArtifactPath,
  withHostTheme,
} from './artifactUtils';

const WORKING_DIR = '/home/ada/project';

describe('languageFromPath', () => {
  it('maps the scripts an agent actually writes to a Prism language', () => {
    expect(languageFromPath('/w/analysis.R')).toBe('r');
    expect(languageFromPath('/w/run.py')).toBe('python');
    expect(languageFromPath('/w/lib.rs')).toBe('rust');
    expect(languageFromPath('/w/setup.sh')).toBe('bash');
    expect(languageFromPath('/w/query.sql')).toBe('sql');
    expect(languageFromPath('/w/main.go')).toBe('go');
    expect(languageFromPath('/w/app.tsx')).toBe('tsx');
    expect(languageFromPath('/w/conf.yml')).toBe('yaml');
  });

  it('treats R Markdown and Quarto as markdown', () => {
    expect(languageFromPath('/w/report.Rmd')).toBe('markdown');
    expect(languageFromPath('/w/report.qmd')).toBe('markdown');
    expect(languageFromPath('/w/report.md')).toBe('markdown');
  });

  it('falls back to the mime type, then plain text', () => {
    expect(languageFromPath('/w/noext', 'application/json')).toBe('json');
    expect(languageFromPath('/w/noext')).toBe('text');
  });
});

describe('languageLabel', () => {
  it('names languages the way people write them', () => {
    expect(languageLabel('/w/analysis.R')).toBe('R');
    expect(languageLabel('/w/report.Rmd')).toBe('R Markdown');
    expect(languageLabel('/w/a.cpp')).toBe('C++');
    expect(languageLabel('/w/q.sql')).toBe('SQL');
    expect(languageLabel('/w/x.py')).toBe('Python');
    expect(languageLabel('/w/notes.txt')).toBe('Text');
  });

  it('keeps acronyms uppercase rather than title-casing them', () => {
    expect(languageLabel('/w/genes.csv')).toBe('CSV');
    expect(languageLabel('/w/genes.tsv')).toBe('TSV');
    expect(languageLabel('/w/a.json')).toBe('JSON');
    expect(languageLabel('/w/a.yaml')).toBe('YAML');
    expect(languageLabel('/w/a.yml')).toBe('YAML');
    expect(languageLabel('/w/a.xml')).toBe('XML');
    expect(languageLabel('/w/a.toml')).toBe('TOML');
    expect(languageLabel('/w/a.css')).toBe('CSS');
    expect(languageLabel('/w/a.html')).toBe('HTML');
  });

  it('spells camel-cased language names correctly', () => {
    expect(languageLabel('/w/app.ts')).toBe('TypeScript');
    expect(languageLabel('/w/app.tsx')).toBe('TSX');
    expect(languageLabel('/w/app.js')).toBe('JavaScript');
    expect(languageLabel('/w/app.jsx')).toBe('JSX');
    expect(languageLabel('/w/run.sh')).toBe('Shell');
    expect(languageLabel('/w/notes.md')).toBe('Markdown');
    expect(languageLabel('/w/lib.rs')).toBe('Rust');
    expect(languageLabel('/w/main.go')).toBe('Go');
  });
});

describe('parseDelimitedTable', () => {
  it('splits a simple CSV', () => {
    expect(parseDelimitedTable('a,b\n1,2\n', ',')).toEqual([
      ['a', 'b'],
      ['1', '2'],
    ]);
  });

  it('keeps a delimiter inside a quoted field', () => {
    expect(parseDelimitedTable('gene,note\n"TP53, alias",x\n', ',')).toEqual([
      ['gene', 'note'],
      ['TP53, alias', 'x'],
    ]);
  });

  it('unescapes doubled quotes', () => {
    expect(parseDelimitedTable('a\n"say ""hi"""\n', ',')).toEqual([['a'], ['say "hi"']]);
  });

  it('handles CRLF line endings and TSV', () => {
    expect(parseDelimitedTable('a\tb\r\n1\t2\r\n', '\t')).toEqual([
      ['a', 'b'],
      ['1', '2'],
    ]);
  });

  it('drops trailing blank lines', () => {
    expect(parseDelimitedTable('a,b\n1,2\n\n', ',')).toEqual([
      ['a', 'b'],
      ['1', '2'],
    ]);
  });
});

describe('baseToolName', () => {
  it('strips the extension prefix', () => {
    expect(baseToolName('developer__text_editor')).toBe('text_editor');
    expect(baseToolName('text_editor')).toBe('text_editor');
    expect(baseToolName('a__b__shell')).toBe('shell');
  });
});

describe('resolveArtifactPath', () => {
  it('keeps absolute and home-relative paths', () => {
    expect(resolveArtifactPath('/tmp/report.md')).toBe('/tmp/report.md');
    expect(resolveArtifactPath('~/notes.md')).toBe('~/notes.md');
  });

  it('anchors relative paths to the working directory', () => {
    expect(resolveArtifactPath('results/plot.png', WORKING_DIR)).toBe(
      '/home/ada/project/results/plot.png'
    );
    expect(resolveArtifactPath('./out.csv', WORKING_DIR)).toBe('/home/ada/project/out.csv');
  });

  it('refuses relative paths with nothing to anchor them to', () => {
    expect(resolveArtifactPath('results/plot.png')).toBeNull();
  });

  it('refuses paths that escape the working directory', () => {
    expect(resolveArtifactPath('../secrets.md', WORKING_DIR)).toBeNull();
    expect(resolveArtifactPath('..\\secrets.md', WORKING_DIR)).toBeNull();
    expect(resolveArtifactPath('..', WORKING_DIR)).toBeNull();
  });

  it('keeps Windows absolute paths intact instead of gluing them to the working dir', () => {
    expect(resolveArtifactPath('C:\\Users\\me\\report.md', 'C:\\Users\\me\\proj')).toBe(
      'C:\\Users\\me\\report.md'
    );
    expect(resolveArtifactPath('C:/Users/me/report.md', 'C:\\proj')).toBe('C:/Users/me/report.md');
    expect(resolveArtifactPath('\\\\server\\share\\a.md', 'C:\\proj')).toBe(
      '\\\\server\\share\\a.md'
    );
  });

  it('joins relative paths with the working directory separator', () => {
    expect(resolveArtifactPath('docs\\out.md', 'C:\\Users\\me\\proj')).toBe(
      'C:\\Users\\me\\proj\\docs\\out.md'
    );
    expect(resolveArtifactPath('.\\out.md', 'C:\\proj')).toBe('C:\\proj\\out.md');
  });

  it('unwraps file:// urls and surrounding quotes', () => {
    expect(resolveArtifactPath('file:///tmp/a%20b.md')).toBe('/tmp/a b.md');
    expect(resolveArtifactPath('"/tmp/quoted.md"')).toBe('/tmp/quoted.md');
  });
});

describe('fileArtifactPathsFromToolCall — text_editor', () => {
  const call = (args: Record<string, unknown>) =>
    fileArtifactPathsFromToolCall('developer__text_editor', args, WORKING_DIR);

  it('previews a written markdown report', () => {
    expect(call({ command: 'write', path: '/home/ada/project/report.md' })).toEqual([
      '/home/ada/project/report.md',
    ]);
  });

  it('previews a written R script', () => {
    expect(call({ command: 'write', path: '/home/ada/project/analysis.R' })).toEqual([
      '/home/ada/project/analysis.R',
    ]);
  });

  it('previews edits, inserts and diffs, not just fresh writes', () => {
    for (const command of ['str_replace', 'insert', 'create', 'diff']) {
      expect(call({ command, path: '/tmp/notes.md' })).toEqual(['/tmp/notes.md']);
    }
  });

  it('does not preview a file the agent merely viewed', () => {
    expect(call({ command: 'view', path: '/tmp/notes.md' })).toEqual([]);
  });

  it('does not preview an undo', () => {
    expect(call({ command: 'undo_edit', path: '/tmp/notes.md' })).toEqual([]);
  });

  it('accepts the file_path alias some models emit', () => {
    expect(call({ command: 'write', file_path: '/tmp/x.csv' })).toEqual(['/tmp/x.csv']);
  });

  it('anchors a relative path to the working directory', () => {
    expect(call({ command: 'write', path: 'docs/summary.md' })).toEqual([
      '/home/ada/project/docs/summary.md',
    ]);
  });

  it('ignores calls with no path at all', () => {
    expect(call({ command: 'write' })).toEqual([]);
  });
});

describe('fileArtifactPathsFromToolCall — other writing tools', () => {
  it('handles write_file and create_file', () => {
    expect(fileArtifactPathsFromToolCall('write_file', { path: '/tmp/a.md' }, WORKING_DIR)).toEqual(
      ['/tmp/a.md']
    );
    expect(
      fileArtifactPathsFromToolCall('create_file', { filename: '/tmp/b.csv' }, WORKING_DIR)
    ).toEqual(['/tmp/b.csv']);
  });

  it('ignores tools that do not write files', () => {
    expect(
      fileArtifactPathsFromToolCall('developer__list_files', { path: '/tmp/a.md' }, WORKING_DIR)
    ).toEqual([]);
    expect(
      fileArtifactPathsFromToolCall('autovisualiser__show_chart', { data: {} }, WORKING_DIR)
    ).toEqual([]);
  });

  it('ignores non-object arguments', () => {
    expect(fileArtifactPathsFromToolCall('write_file', 'oops', WORKING_DIR)).toEqual([]);
    expect(fileArtifactPathsFromToolCall('write_file', null, WORKING_DIR)).toEqual([]);
  });
});

describe('fileArtifactPathsFromToolCall — shell', () => {
  const shell = (command: string) =>
    fileArtifactPathsFromToolCall('developer__shell', { command }, WORKING_DIR);

  it('catches a redirect target', () => {
    expect(shell('python summarize.py > results/summary.csv')).toEqual([
      '/home/ada/project/results/summary.csv',
    ]);
  });

  it('catches an append redirect', () => {
    expect(shell('echo hi >> log.txt')).toEqual(['/home/ada/project/log.txt']);
  });

  it('catches conventional output flags', () => {
    expect(shell('Rscript plot.R -o figure.png')).toEqual(['/home/ada/project/figure.png']);
    expect(shell('pandoc a.md --output report.html')).toEqual(['/home/ada/project/report.html']);
  });

  it('handles quoted output paths with spaces', () => {
    expect(shell('convert in.png -o "my figures/out.png"')).toEqual([
      '/home/ada/project/my figures/out.png',
    ]);
  });

  it('ignores stderr redirection and other non-file targets', () => {
    expect(shell('make 2>&1')).toEqual([]);
    expect(shell('cat a.md > /dev/null')).toEqual([]);
  });

  it('does not turn a plain listing into artifacts', () => {
    expect(shell('ls -la')).toEqual([]);
    expect(shell('cat report.md')).toEqual([]);
  });

  it('ignores redirect targets with no previewable extension', () => {
    expect(shell('./build > out.bin')).toEqual([]);
  });

  it('finds several outputs in one command', () => {
    expect(shell('a.sh > one.csv; b.sh > two.csv')).toEqual([
      '/home/ada/project/one.csv',
      '/home/ada/project/two.csv',
    ]);
  });
});

describe('withHostTheme', () => {
  it('injects the host theme right after <head> so it runs before the figure runtime', () => {
    const html = '<!doctype html><html><head><script>/*common*/</script></head><body></body></html>';
    const out = withHostTheme(html, 'light');
    expect(out).toContain('<head><script>window.__BR_VIZ_HOST_THEME__="light";</script>');
    // ...and it precedes the figure's own runtime script.
    expect(out.indexOf('__BR_VIZ_HOST_THEME__')).toBeLessThan(out.indexOf('/*common*/'));
  });

  it('carries dark through and falls back to a prefix when there is no <head>', () => {
    expect(withHostTheme('<html><head></head></html>', 'dark')).toContain(
      '__BR_VIZ_HOST_THEME__="dark"'
    );
    expect(withHostTheme('<div>no head</div>', 'light')).toBe(
      '<script>window.__BR_VIZ_HOST_THEME__="light";</script><div>no head</div>'
    );
  });
});
