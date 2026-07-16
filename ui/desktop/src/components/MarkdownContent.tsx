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

import { Check, Copy } from './icons';
import { wrapHTMLInCodeBlock } from '../utils/htmlSecurity';
import type { ArtifactSource } from './artifacts/artifactTypes';
import {
  basenameFromPath,
  looksLikePreviewableFile,
  pathFromArtifactHref,
  resolveArtifactPath,
} from './artifacts/artifactUtils';

interface CodeProps extends React.ClassAttributes<HTMLElement>, React.HTMLAttributes<HTMLElement> {
  inline?: boolean;
  onOpenArtifact?: (artifact: ArtifactSource) => void;
  workingDir?: string;
}

interface MarkdownContentProps {
  content: string;
  className?: string;
  onOpenArtifact?: (artifact: ArtifactSource) => void;
  workingDir?: string;
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
    <div className="w-full border border-border-subtle rounded-xl overflow-hidden my-2 bg-background-muted">
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
  if (
    (/^file:\/\//i.test(value) || /^[a-z]:[\\/]/i.test(value) || value.startsWith('\\\\')) &&
    looksLikePreviewableFile(value)
  ) {
    return value;
  }
  return defaultUrlTransform(value);
}

function artifactSourceFromMarkdownValue(
  value: string,
  workingDir?: string
): ArtifactSource | null {
  const candidate = value.trim();
  if (!candidate || candidate.includes('\n') || candidate.includes('\r')) return null;
  if (LOOPBACK_URL_RE.test(candidate)) {
    return { kind: 'externalUrl', title: candidate, url: candidate };
  }
  if (!looksLikePreviewableFile(candidate)) return null;
  const rawPath = pathFromArtifactHref(candidate);
  const path = resolveArtifactPath(rawPath, workingDir) ?? rawPath;
  return { kind: 'file', title: basenameFromPath(path), path };
}

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
    <button
      type="button"
      className={`inline cursor-pointer break-all rounded-sm text-left underline decoration-border-strong underline-offset-2 transition-colors hover:text-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-border-focus ${
        inlineCode
          ? 'bg-inline-code px-1 py-0.5 font-mono text-[0.9em] text-text-default hover:bg-background-medium'
          : 'font-mono text-[0.95em] text-text-default'
      }`}
      onClick={() => onOpenArtifact(artifact)}
      title={`Preview ${artifact.title} in the side panel`}
    >
      {children}
    </button>
  );
}

const MarkdownCode = memo(
  React.forwardRef(function MarkdownCode(
    { inline, className, children, onOpenArtifact, workingDir, ...props }: CodeProps,
    ref: React.Ref<HTMLElement>
  ) {
    const match = /language-(\w+)/.exec(className || '');
    const text = String(children);
    const artifact = !match ? artifactSourceFromMarkdownValue(text, workingDir) : null;
    return !inline && match ? (
      <CodeBlock language={match[1]}>{text.replace(/\n$/, '')}</CodeBlock>
    ) : artifact && onOpenArtifact ? (
      <ArtifactLinkButton artifact={artifact} onOpenArtifact={onOpenArtifact} inlineCode>
        {children}
      </ArtifactLinkButton>
    ) : (
      <code ref={ref} {...props} className="break-all bg-inline-code whitespace-pre-wrap font-mono">
        {children}
      </code>
    );
  })
);

// File paths the assistant mentions in prose aren't markdown links, so
// ReactMarkdown renders them as plain text. Keep the match narrow: an absolute,
// home-relative, or multi-segment relative path with a filename extension.
const FILE_PATH_RE =
  /(?<![\w:/\\@.])((?:file:\/\/|~\/|\/|[A-Za-z]:[\\/]|(?:[\w.\-+@%]+[\\/])+)[\w.\-+@%/\\]*\.[A-Za-z0-9]{1,12})(?![\w/\\])/g;

function linkifyFilePaths(
  children: React.ReactNode,
  onOpenArtifact?: (artifact: ArtifactSource) => void,
  workingDir?: string
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
      const artifact = artifactSourceFromMarkdownValue(filePath, workingDir);
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
  ...props
}: React.HTMLAttributes<globalThis.HTMLParagraphElement> & {
  onOpenArtifact?: (artifact: ArtifactSource) => void;
  workingDir?: string;
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
  if (isDisplayMath) {
    return (
      <p className="flex justify-center my-3 overflow-x-auto" {...props}>
        {children}
      </p>
    );
  }
  return <p {...props}>{linkifyFilePaths(children, onOpenArtifact, workingDir)}</p>;
};

const MarkdownContent = memo(function MarkdownContent({
  content,
  className = '',
  onOpenArtifact,
  workingDir,
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
      prose-code:break-words prose-code:whitespace-pre-wrap prose-code:font-mono
      prose-code:text-text-default prose-code:bg-background-medium
      prose-code:rounded-sm prose-code:px-1 prose-code:py-0.5 prose-code:text-[0.9em]
      prose-code:before:content-none prose-code:after:content-none
      prose-a:break-all prose-a:overflow-wrap-anywhere
      prose-table:table prose-table:w-full
      prose-blockquote:text-text-muted prose-blockquote:border-border-subtle prose-blockquote:not-italic
      prose-td:border prose-td:border-border-subtle prose-td:p-2 prose-td:align-top
      prose-th:border prose-th:border-border-subtle prose-th:p-2 prose-th:text-text-default
      prose-thead:bg-background-medium
      prose-h1:text-lg prose-h1:font-semibold prose-h1:mb-3 prose-h1:mt-0 prose-h1:font-sans
      prose-h2:text-base prose-h2:font-semibold prose-h2:mb-2 prose-h2:mt-4 prose-h2:font-sans
      prose-h3:text-sm prose-h3:font-semibold prose-h3:mb-2 prose-h3:mt-3 prose-h3:font-sans
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
            const artifactPath =
              href && looksLikePreviewableFile(href) ? pathFromArtifactHref(href) : null;
            if (artifactPath && onOpenArtifact) {
              const resolvedPath = resolveArtifactPath(artifactPath, workingDir) ?? artifactPath;
              return (
                <ArtifactLinkButton
                  artifact={{
                    kind: 'file',
                    title: basenameFromPath(resolvedPath),
                    path: resolvedPath,
                  }}
                  onOpenArtifact={onOpenArtifact}
                >
                  {children}
                </ArtifactLinkButton>
              );
            }
            // Loopback/app URLs (the daemon serves BioRouter apps on 127.0.0.1)
            // can be framed, so preview them inline in the side panel. Public
            // websites almost always send X-Frame-Options / frame-ancestors and
            // would render as a BLANK iframe, so those open in the real browser
            // (via the <a> below) where they actually load.
            const isLoopbackUrl = !!href && LOOPBACK_URL_RE.test(href);
            if (isLoopbackUrl && onOpenArtifact) {
              return (
                <button
                  type="button"
                  className="inline cursor-pointer break-all text-left text-text-accent underline underline-offset-2 decoration-border-strong hover:text-text-muted"
                  onClick={() => onOpenArtifact({ kind: 'externalUrl', title: href, url: href })}
                >
                  {children}
                </button>
              );
            }
            return (
              <a href={href} {...props} target="_blank" rel="noopener noreferrer">
                {children}
              </a>
            );
          },
          code: ({ node: _node, ...props }) => (
            <MarkdownCode {...props} onOpenArtifact={onOpenArtifact} workingDir={workingDir} />
          ),
          p: ({ node: _node, ...props }) => (
            <MarkdownParagraph {...props} onOpenArtifact={onOpenArtifact} workingDir={workingDir} />
          ),
          li: ({ children, node: _node, ...props }) => (
            <li {...props}>{linkifyFilePaths(children, onOpenArtifact, workingDir)}</li>
          ),
          td: ({ children, node: _node, ...props }) => (
            <td {...props}>{linkifyFilePaths(children, onOpenArtifact, workingDir)}</td>
          ),
          th: ({ children, node: _node, ...props }) => (
            <th {...props}>{linkifyFilePaths(children, onOpenArtifact, workingDir)}</th>
          ),
        }}
      >
        {processedContent}
      </ReactMarkdown>
    </div>
  );
});

export default MarkdownContent;
