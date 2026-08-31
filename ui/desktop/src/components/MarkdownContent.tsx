import React, { useState, useEffect, useRef, memo, useMemo } from 'react';
import ReactMarkdown, { defaultUrlTransform } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkBreaks from 'remark-breaks';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import 'katex/dist/katex.min.css';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { useResolvedTheme, useThemeFamily } from '../contexts/ThemeContext';
import {
  CODE_FONT_FAMILY,
  CODE_FONT_SIZE,
  CODE_LINE_HEIGHT,
  codeThemesByFamily,
} from '../styles/codeTheme';
import { Button } from './ui/button';

import { Check, Copy, Image as ImageIcon } from './icons/app-icons';
import { wrapHTMLInCodeBlock } from '../utils/htmlSecurity';
import { normalizeExternalHttpUrl } from '../utils/externalUrl';
import type { ArtifactFilePreview, ArtifactSource } from './artifacts/artifactTypes';
import {
  imageSourceForPreview,
  looksLikePreviewableFile,
  resolveMarkdownImageSource,
} from './artifacts/artifactUtils';
import {
  isLocalFileReference,
  localFileBasename,
  resolveFileLink,
  type KnownFilePaths,
} from './artifacts/artifactFileLinks';

interface CodeProps extends React.ClassAttributes<HTMLElement>, React.HTMLAttributes<HTMLElement> {
  inline?: boolean;
  onOpenArtifact?: (artifact: ArtifactSource) => void;
  workingDir?: string;
  knownFilePaths?: KnownFilePaths;
}

interface MarkdownContentProps {
  content: string;
  className?: string;
  onOpenArtifact?: (artifact: ArtifactSource) => void;
  workingDir?: string;
  knownFilePaths?: KnownFilePaths;
}

// Memoized CodeBlock component to prevent re-rendering when props haven't changed
const CodeBlock = memo(function CodeBlock({
  language,
  children,
}: {
  language: string;
  children: string;
}) {
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef<number | null>(null);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(children);
      setCopied(true);
      if (timeoutRef.current) window.clearTimeout(timeoutRef.current);
      timeoutRef.current = window.setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy text: ', err);
    }
  };

  useEffect(() => {
    return () => {
      if (timeoutRef.current) window.clearTimeout(timeoutRef.current);
    };
  }, []);

  const codeStyle = codeThemesByFamily[useThemeFamily()][useResolvedTheme()];

  const memoizedSyntaxHighlighter = useMemo(() => {
    return (
      <SyntaxHighlighter
        style={codeStyle}
        language={language}
        PreTag="div"
        customStyle={{
          margin: 0,
          padding: '12px',
          background: 'transparent',
          width: '100%',
          maxWidth: '100%',
        }}
        codeTagProps={{
          style: {
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
            overflowWrap: 'break-word',
            fontFamily: CODE_FONT_FAMILY,
            fontSize: CODE_FONT_SIZE,
            lineHeight: CODE_LINE_HEIGHT,
          },
        }}
        showLineNumbers={false}
        wrapLines={false}
        lineProps={undefined}
      >
        {children}
      </SyntaxHighlighter>
    );
  }, [codeStyle, language, children]);

  return (
    // `bg-background-code`, not `bg-background-muted`: the syntax palette in
    // codeTheme.ts is verified against --background-code (#f5f5f3 / #1b1b19,
    // design.md §5.1) and --background-muted is a different surface (#f4f4f2 /
    // #232320) — so painting muted would put dark code on a ground its palette
    // was never measured on, which is how `comment` once fell to 4.15:1, under
    // AA. The two tokens are close now that the neutrals are shared, but they
    // are still distinct and the generator measures against the code one.
    // The highlighter itself renders transparent, so this div IS the ground the
    // reader sees.
    <div className="w-full border border-border-subtle rounded-xl overflow-hidden my-2 bg-background-code">
      {/* Header bar */}
      <div className="flex items-center justify-between h-8 px-3 bg-background-default border-b border-border-subtle">
        <span className="text-[11px] font-medium text-text-subtle uppercase tracking-wider select-none">
          {language || 'code'}
        </span>
        <Button
          variant="ghost"
          size="xs"
          onClick={handleCopy}
          className="gap-1 text-[11px] text-text-muted hover:text-text-default"
          title="Copy code"
        >
          {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
          <span>{copied ? 'Copied' : 'Copy'}</span>
        </Button>
      </div>
      {/* Code body */}
      <div className="w-full overflow-x-auto">{memoizedSyntaxHighlighter}</div>
    </div>
  );
});

const LOOPBACK_URL_RE =
  /^https?:\/\/(localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\])(?::\d+)?(?:[/?#]|$)/i;

function artifactAwareUrlTransform(value: string) {
  if (isLocalFileReference(value) && looksLikePreviewableFile(value)) {
    return value;
  }
  return defaultUrlTransform(value);
}

function previewableExternalUrl(href: string): string | null {
  try {
    return normalizeExternalHttpUrl(href);
  } catch {
    return null;
  }
}

function artifactSourceFromMarkdownValue(
  value: string,
  workingDir?: string,
  knownFilePaths?: KnownFilePaths
): ArtifactSource | null {
  const candidate = value.trim();
  if (!candidate || candidate.includes('\n') || candidate.includes('\r')) return null;
  if (LOOPBACK_URL_RE.test(candidate)) {
    return { kind: 'externalUrl', title: candidate, url: candidate };
  }
  if (!looksLikePreviewableFile(candidate)) return null;
  const resolved = resolveFileLink(candidate, workingDir, knownFilePaths);
  if (resolved.kind === 'unresolved') return null;
  return {
    kind: 'file',
    title: localFileBasename(resolved.path),
    path: resolved.path,
    ...(resolved.line ? { line: resolved.line } : {}),
  };
}

// The ONE link treatment in this renderer (design spec "The markdown layer,
// rebuilt"). Plain `<a>` gets it from the typography plugin, whose
// `--tw-prose-links` main.css points at `--text-accent`; the two <button>-based
// links below are not `<a>`, so the plugin cannot reach them and they restate it
// here. Previously these were three different treatments (plugin accent,
// `decoration-border-strong` with default ink, and accent-with-neutral-underline).
const LINK_CLASS =
  'cursor-pointer font-medium text-text-accent underline decoration-text-accent/40 underline-offset-2 transition-colors hover:decoration-text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-border-focus';

function ArtifactLinkButton({
  artifact,
  children,
  onOpenArtifact,
  inlineCode = false,
}: {
  artifact: ArtifactSource;
  children: React.ReactNode;
  onOpenArtifact: (artifact: ArtifactSource) => void;
  inlineCode?: boolean;
}) {
  return (
    // Both variants are mono at 13px — the same size as a fenced block
    // (CODE_FONT_SIZE) and inline code. The sole difference is the inline-code
    // fill, which is the only thing `inlineCode` should mean; the two variants
    // used to also disagree on font-size (0.9em vs 0.95em) for no stated reason.
    <button
      type="button"
      className={`inline break-all rounded-sm text-left font-mono text-[13px] ${LINK_CLASS} ${
        inlineCode ? 'bg-background-medium px-1 py-0.5' : ''
      }`}
      onClick={() => onOpenArtifact(artifact)}
      title={`Preview ${artifact.title} in the side panel`}
    >
      {children}
    </button>
  );
}

// A local image whose bytes could not be read (denied by the main-process
// allowlist, missing, or not an image) and a remote image that failed to load
// both collapse to this inline placeholder instead of a dead <img> with a
// busted src. The alt text stays legible so the reader still knows what was
// meant to be here.
function BrokenImage({ alt }: { alt?: string }) {
  const label = alt?.trim() || 'Image unavailable';
  return (
    <span
      role="img"
      aria-label={label}
      title={alt?.trim() ? `Image unavailable: ${alt}` : 'Image unavailable'}
      className="inline-flex items-center gap-1 rounded-sm border border-border-subtle bg-background-medium px-1.5 py-0.5 align-middle text-[12px] text-text-muted"
    >
      <ImageIcon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      {label}
    </span>
  );
}

// Markdown images in the preview. Remote (`http(s)`/`data:`) srcs render
// directly — the same reach chat already has. A LOCAL image (relative to the
// previewed file, absolute, `~`, or `file://`) can't be loaded by the renderer
// from disk, so it is read through the existing allowlisted `readArtifactFile`
// IPC and inlined as a `data:` URI (CSP-safe). Anything the allowlist denies, or
// that traverses out of the file's directory, degrades to `BrokenImage`.
const MarkdownImage = memo(function MarkdownImage({
  src,
  alt,
  workingDir,
}: {
  src?: string;
  alt?: string;
  workingDir?: string;
}) {
  const source = useMemo(
    () => resolveMarkdownImageSource(src ?? '', workingDir),
    [src, workingDir]
  );
  const [resolvedSrc, setResolvedSrc] = useState<string | null>(
    source.kind === 'remote' ? source.url : null
  );
  const [failed, setFailed] = useState(source.kind === 'blocked');

  useEffect(() => {
    if (source.kind === 'remote') {
      setResolvedSrc(source.url);
      setFailed(false);
      return;
    }
    if (source.kind === 'blocked') {
      setResolvedSrc(null);
      setFailed(true);
      return;
    }

    let cancelled = false;
    // A large image arrives as bytes and becomes a `blob:` URL that has to be
    // revoked, so the cleanup below owns whatever this effect minted.
    let revokeSrc: (() => void) | null = null;
    setResolvedSrc(null);
    setFailed(false);
    const read = window.electron?.readArtifactFile;
    if (!read) {
      setFailed(true);
      return;
    }
    void read(source.path)
      .then((preview: ArtifactFilePreview) => {
        if (cancelled) return;
        if (preview && preview.kind === 'image') {
          const { src, revoke } = imageSourceForPreview(preview);
          if (!src) {
            setFailed(true);
            revoke();
            return;
          }
          revokeSrc = revoke;
          setResolvedSrc(src);
        } else {
          setFailed(true);
        }
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
      revokeSrc?.();
    };
  }, [source]);

  if (failed) return <BrokenImage alt={alt} />;
  if (!resolvedSrc) {
    return (
      <span
        aria-label={alt?.trim() || 'Loading image'}
        className="inline-block h-4 w-24 animate-pulse rounded-sm bg-background-medium align-middle"
      />
    );
  }
  return (
    <img
      src={resolvedSrc}
      alt={alt ?? ''}
      className="mx-auto my-2 h-auto max-w-full rounded-md"
      onError={() => setFailed(true)}
    />
  );
});

// External links open in the SYSTEM browser through the existing IPC — never by
// navigating the renderer/panel (a top-frame navigation would drop the CSP and
// keep the preload bridge). target=_blank alone leans on the main process's
// window-open handler; calling openExternal here makes it explicit and testable.
function openExternalLink(event: React.MouseEvent<HTMLAnchorElement>, href: string) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
    return;
  }
  const opener = window.electron?.openExternal;
  if (!opener) return;
  event.preventDefault();
  void opener(href);
}

const MarkdownCode = memo(
  React.forwardRef(function MarkdownCode(
    {
      inline,
      className,
      children,
      onOpenArtifact,
      workingDir,
      knownFilePaths,
      ...props
    }: CodeProps,
    ref: React.Ref<HTMLElement>
  ) {
    const match = /language-(\w+)/.exec(className || '');
    const text = String(children);
    const artifact = !match
      ? artifactSourceFromMarkdownValue(text, workingDir, knownFilePaths)
      : null;
    return !inline && match ? (
      <CodeBlock language={match[1]}>{text.replace(/\n$/, '')}</CodeBlock>
    ) : artifact && onOpenArtifact ? (
      <ArtifactLinkButton artifact={artifact} onOpenArtifact={onOpenArtifact} inlineCode>
        {children}
      </ArtifactLinkButton>
    ) : (
      // Fill/size/padding come from the `prose-code:*` list on the wrapper —
      // the single inline-code recipe. This used to also carry `bg-inline-code`,
      // a second, competing fill that only won via a specificity ladder in
      // main.css.
      <code ref={ref} {...props} className="break-all whitespace-pre-wrap font-mono">
        {children}
      </code>
    );
  })
);

// File paths the assistant mentions in prose aren't markdown links, so
// ReactMarkdown renders them as plain text. Keep the match narrow: an absolute,
// home-relative, or multi-segment relative path with a filename extension.
const FILE_PATH_RE =
  /(?<![^\s(\[{])((?:file:\/\/|~\/|\/|[A-Za-z]:[\\/]|(?:[\p{L}\p{N}\p{M}.\-+@%]+[\\/])+)[^\s)\]}\x60"'<>]*\.[A-Za-z0-9]{1,12}(?::\d+|#L\d+|%[^\s)\]}\x60"'<>.,!?;]*)?)(?=$|[\s)\]},;]|[.!?](?=$|[\s)\]},;]))/gu;

function linkifyFilePaths(
  children: React.ReactNode,
  onOpenArtifact?: (artifact: ArtifactSource) => void,
  workingDir?: string,
  knownFilePaths?: KnownFilePaths
): React.ReactNode {
  if (!onOpenArtifact) return children;
  return React.Children.map(children, (child) => {
    if (typeof child !== 'string' || !child.includes('/')) return child;
    const out: React.ReactNode[] = [];
    let last = 0;
    let match: RegExpExecArray | null;
    FILE_PATH_RE.lastIndex = 0;
    while ((match = FILE_PATH_RE.exec(child)) !== null) {
      const filePath = match[1];
      if (match.index > last) out.push(child.slice(last, match.index));
      const artifact = artifactSourceFromMarkdownValue(filePath, workingDir, knownFilePaths);
      if (!artifact) {
        out.push(filePath);
        last = match.index + filePath.length;
        continue;
      }
      out.push(
        <ArtifactLinkButton key={match.index} artifact={artifact} onOpenArtifact={onOpenArtifact}>
          {filePath}
        </ArtifactLinkButton>
      );
      last = match.index + filePath.length;
    }
    if (last === 0) return child;
    if (last < child.length) out.push(child.slice(last));
    return out;
  });
}

const MarkdownParagraph = ({
  children,
  onOpenArtifact,
  workingDir,
  knownFilePaths,
  ...props
}: React.HTMLAttributes<globalThis.HTMLParagraphElement> & {
  onOpenArtifact?: (artifact: ArtifactSource) => void;
  workingDir?: string;
  knownFilePaths?: KnownFilePaths;
}) => {
  const childArray = React.Children.toArray(children);
  const meaningfulChildren = childArray.filter(
    (child) => !(typeof child === 'string' && child.trim() === '')
  );
  const isDisplayMath =
    meaningfulChildren.length === 1 &&
    React.isValidElement(meaningfulChildren[0]) &&
    typeof (meaningfulChildren[0] as React.ReactElement<{ className?: string }>).props
      ?.className === 'string' &&
    (meaningfulChildren[0] as React.ReactElement<{ className?: string }>).props.className!.includes(
      'katex'
    );
  // Centring, margin and overflow for math live in ONE place: the
  // `.katex-display` block in main.css. This wrapper used to restate all three
  // as `flex justify-center my-3 overflow-x-auto`.
  //
  // Note it never actually reached *display* math: remark-math emits `$$…$$` as
  // a block sibling, so `.katex-display` is a direct child of the prose root and
  // is never inside a <p>. The duplicate styling only ever landed on a paragraph
  // holding nothing but INLINE math — which `.katex-display` does not style, so
  // the flex/margin were wrong there too. What this branch is genuinely for is
  // keeping `linkifyFilePaths` off KaTeX's output.
  if (isDisplayMath) {
    return <p {...props}>{children}</p>;
  }
  return <p {...props}>{linkifyFilePaths(children, onOpenArtifact, workingDir, knownFilePaths)}</p>;
};

const MarkdownContent = memo(function MarkdownContent({
  content,
  className = '',
  onOpenArtifact,
  workingDir,
  knownFilePaths,
}: MarkdownContentProps) {
  const [processedContent, setProcessedContent] = useState(content);

  useEffect(() => {
    try {
      const processed = wrapHTMLInCodeBlock(content);
      setProcessedContent(processed);
    } catch (error) {
      console.error('Error processing content:', error);
      setProcessedContent(content);
    }
  }, [content]);

  return (
    <div
      className={`w-full overflow-x-hidden prose prose-sm text-text-default dark:prose-invert max-w-full word-break font-sans
      prose-pre:p-0 prose-pre:m-0 prose-pre:bg-transparent prose-pre:rounded-none !p-0
      prose-pre:[&:has(>code)]:p-3 prose-pre:[&>code]:p-0
      prose-code:break-words prose-code:whitespace-pre-wrap prose-code:font-mono
      prose-code:text-text-default prose-code:bg-background-medium
      prose-code:rounded-sm prose-code:px-1 prose-code:py-0.5
      prose-code:text-[13px] prose-code:font-normal prose-code:not-italic
      prose-code:before:content-none prose-code:after:content-none
      prose-a:break-all prose-a:font-medium prose-a:underline
      prose-a:decoration-text-accent/40 prose-a:underline-offset-2
      prose-table:table prose-table:w-full prose-table:text-[13px]
      prose-th:tabular-nums prose-td:tabular-nums
      prose-blockquote:text-text-muted prose-blockquote:border-border-subtle prose-blockquote:not-italic
      [&_blockquote_p:first-of-type]:before:content-none
      [&_blockquote_p:last-of-type]:after:content-none
      prose-h1:text-[18px] prose-h1:leading-[26px] prose-h1:font-semibold prose-h1:tracking-[-0.005em] prose-h1:mb-3 prose-h1:mt-0 prose-h1:font-sans
      prose-h2:text-[16px] prose-h2:leading-[24px] prose-h2:font-semibold prose-h2:mb-2 prose-h2:mt-4 prose-h2:font-sans
      prose-h3:text-[15px] prose-h3:leading-[22px] prose-h3:font-semibold prose-h3:mb-2 prose-h3:mt-3 prose-h3:font-sans
      prose-h4:text-[13px] prose-h4:leading-[18px] prose-h4:font-semibold prose-h4:tracking-[0.02em] prose-h4:text-text-muted prose-h4:mb-1 prose-h4:mt-3 prose-h4:font-sans
      prose-p:mt-0 prose-p:mb-2 prose-p:font-sans
      prose-ol:my-2 prose-ol:font-sans
      prose-ul:mt-0 prose-ul:mb-3 prose-ul:font-sans
      prose-li:m-0 prose-li:font-sans ${className}`}
    >
      <ReactMarkdown
        urlTransform={artifactAwareUrlTransform}
        remarkPlugins={[remarkGfm, remarkBreaks, [remarkMath, { singleDollarTextMath: false }]]}
        rehypePlugins={[
          [
            rehypeKatex,
            {
              throwOnError: false,
              // KaTeX takes a raw colour string, not a CSS var. Keep it in step
              // with --text-danger (light).
              errorColor: '#b3261e',
              strict: false,
            },
          ],
        ]}
        components={{
          a: ({ href, children, node: _node, ...props }) => {
            if (!href) return <>{children}</>;
            if (isLocalFileReference(href)) {
              // A link to a sibling/local file. If there is a panel to open it in,
              // preview it there; otherwise render it as styled, inert text with a
              // tooltip rather than an <a> that would dead-navigate the renderer.
              const resolved = resolveFileLink(href, workingDir, knownFilePaths);
              if (
                onOpenArtifact &&
                looksLikePreviewableFile(href) &&
                resolved.kind === 'resolved'
              ) {
                return (
                  <ArtifactLinkButton
                    artifact={{
                      kind: 'file',
                      title: localFileBasename(resolved.path),
                      path: resolved.path,
                      ...(resolved.line ? { line: resolved.line } : {}),
                    }}
                    onOpenArtifact={onOpenArtifact}
                  >
                    {children}
                  </ArtifactLinkButton>
                );
              }
              return (
                <span
                  className="cursor-default font-medium text-text-muted underline decoration-dotted decoration-text-muted/40 underline-offset-2"
                  title={resolved.kind === 'unresolved' ? resolved.reason : resolved.path}
                >
                  {children}
                </span>
              );
            }
            const externalUrl = previewableExternalUrl(href);
            if (externalUrl && onOpenArtifact) {
              return (
                <button
                  type="button"
                  className={`inline break-all text-left ${LINK_CLASS}`}
                  onClick={() =>
                    onOpenArtifact({ kind: 'externalUrl', title: href, url: externalUrl })
                  }
                >
                  {children}
                </button>
              );
            }
            return (
              <a
                href={href}
                {...props}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(event) => openExternalLink(event, href)}
              >
                {children}
              </a>
            );
          },
          img: ({ src, alt, node: _node }) => (
            <MarkdownImage
              src={typeof src === 'string' ? src : undefined}
              alt={typeof alt === 'string' ? alt : undefined}
              workingDir={workingDir}
            />
          ),
          code: ({ node: _node, ...props }) => (
            <MarkdownCode
              {...props}
              onOpenArtifact={onOpenArtifact}
              workingDir={workingDir}
              knownFilePaths={knownFilePaths}
            />
          ),
          p: ({ node: _node, ...props }) => (
            <MarkdownParagraph
              {...props}
              onOpenArtifact={onOpenArtifact}
              workingDir={workingDir}
              knownFilePaths={knownFilePaths}
            />
          ),
          li: ({ children, node: _node, ...props }) => (
            <li {...props}>
              {linkifyFilePaths(children, onOpenArtifact, workingDir, knownFilePaths)}
            </li>
          ),
          td: ({ children, node: _node, ...props }) => (
            <td {...props}>
              {linkifyFilePaths(children, onOpenArtifact, workingDir, knownFilePaths)}
            </td>
          ),
          th: ({ children, node: _node, ...props }) => (
            <th {...props}>
              {linkifyFilePaths(children, onOpenArtifact, workingDir, knownFilePaths)}
            </th>
          ),
        }}
      >
        {processedContent}
      </ReactMarkdown>
    </div>
  );
});

export default MarkdownContent;
