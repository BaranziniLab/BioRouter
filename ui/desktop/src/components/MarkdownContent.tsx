import React, { useState, useEffect, useRef, memo, useMemo } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkBreaks from 'remark-breaks';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import 'katex/dist/katex.min.css';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { oneLight } from 'react-syntax-highlighter/dist/esm/styles/prism';

// Warm-palette light theme — background matches bg-background-medium (#f4f0e6)
const warmLightTheme = {
  ...oneLight,
  'code[class*="language-"]': {
    ...oneLight['code[class*="language-"]'],
    color: '#2a2520',
    background: 'transparent',
    fontSize: '13.5px',
    lineHeight: '1.6',
  },
  'pre[class*="language-"]': {
    ...oneLight['pre[class*="language-"]'],
    color: '#2a2520',
    background: 'transparent',
    fontSize: '13.5px',
    lineHeight: '1.6',
    margin: 0,
    padding: 0,
  },
  comment: { color: '#8a8078', fontStyle: 'italic' },
  prolog: { color: '#8a8078' },
  punctuation: { color: '#5a5248' },
  keyword: { color: '#7c5c2e', fontWeight: '600' },
  'keyword.module': { color: '#7c5c2e', fontWeight: '600' },
  builtin: { color: '#7c5c2e' },
  string: { color: '#4a7c59' },
  number: { color: '#8b4513' },
  boolean: { color: '#8b4513' },
  function: { color: '#3d5a8a' },
  'class-name': { color: '#3d5a8a', fontWeight: '600' },
  operator: { color: '#5a5248' },
  variable: { color: '#2a2520' },
};

import { Check, Copy } from './icons';
import { wrapHTMLInCodeBlock } from '../utils/htmlSecurity';

interface CodeProps extends React.ClassAttributes<HTMLElement>, React.HTMLAttributes<HTMLElement> {
  inline?: boolean;
}

interface MarkdownContentProps {
  content: string;
  className?: string;
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

  const memoizedSyntaxHighlighter = useMemo(() => {
    return (
      <SyntaxHighlighter
        style={warmLightTheme}
        language={language}
        PreTag="div"
        customStyle={{
          margin: 0,
          padding: '8px 12px',
          background: 'transparent',
          width: '100%',
          maxWidth: '100%',
        }}
        codeTagProps={{
          style: {
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
            overflowWrap: 'break-word',
            fontFamily: 'var(--font-mono)',
            fontSize: '13.5px',
            lineHeight: '1.6',
          },
        }}
        showLineNumbers={false}
        wrapLines={false}
        lineProps={undefined}
      >
        {children}
      </SyntaxHighlighter>
    );
  }, [language, children]);

  return (
    <div className="w-full border border-border-subtle rounded-md overflow-hidden my-2 font-sans">
      {/* Header bar */}
      <div className="flex items-center justify-between px-3 py-1 bg-background-medium border-b border-border-subtle">
        <span className="text-[11px] text-text-muted uppercase tracking-wider select-none">
          {language || 'code'}
        </span>
        <button
          onClick={handleCopy}
          className="flex items-center gap-1 px-1.5 py-0.5 rounded text-[11px] text-text-muted hover:text-text-default hover:bg-background-strong transition-colors duration-150"
          title="Copy code"
        >
          {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
          <span>{copied ? 'Copied' : 'Copy'}</span>
        </button>
      </div>
      {/* Code body */}
      <div className="w-full overflow-x-auto bg-background-medium">
        {memoizedSyntaxHighlighter}
      </div>
    </div>
  );
});

const MarkdownCode = memo(
  React.forwardRef(function MarkdownCode(
    { inline, className, children, ...props }: CodeProps,
    ref: React.Ref<HTMLElement>
  ) {
    const match = /language-(\w+)/.exec(className || '');
    return !inline && match ? (
      <CodeBlock language={match[1]}>{String(children).replace(/\n$/, '')}</CodeBlock>
    ) : (
      <code ref={ref} {...props} className="break-all bg-inline-code whitespace-pre-wrap font-mono">
        {children}
      </code>
    );
  })
);

const MarkdownParagraph = ({ children, ...props }: React.HTMLAttributes<globalThis.HTMLParagraphElement>) => {
  const childArray = React.Children.toArray(children);
  const meaningfulChildren = childArray.filter(
    (child) => !(typeof child === 'string' && child.trim() === '')
  );
  const isDisplayMath =
    meaningfulChildren.length === 1 &&
    React.isValidElement(meaningfulChildren[0]) &&
    typeof (meaningfulChildren[0] as React.ReactElement<{ className?: string }>).props?.className ===
      'string' &&
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
  return <p {...props}>{children}</p>;
};

const MarkdownContent = memo(function MarkdownContent({
  content,
  className = '',
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
      prose-code:rounded prose-code:px-1 prose-code:py-0.5 prose-code:text-[0.85em]
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
        remarkPlugins={[remarkGfm, remarkBreaks, [remarkMath, { singleDollarTextMath: true }]]}
        rehypePlugins={[
          [
            rehypeKatex,
            {
              throwOnError: false,
              errorColor: '#cc0000',
              strict: false,
            },
          ],
        ]}
        components={{
          a: ({ ...props }) => <a {...props} target="_blank" rel="noopener noreferrer" />,
          code: MarkdownCode,
          p: MarkdownParagraph,
        }}
      >
        {processedContent}
      </ReactMarkdown>
    </div>
  );
});

export default MarkdownContent;
