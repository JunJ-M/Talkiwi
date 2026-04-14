import { useCallback, useMemo, useState } from "react";
import { useEditorStore } from "../../stores/editorStore";
import { useToastStore } from "../../stores/toastStore";
import { sessionRegenerate } from "../../services/session";

const MODEL_OPTIONS = [
  { id: "local-qwen", label: "Local Qwen (v2.5)" },
  { id: "cloud-claude", label: "Cloud Claude (3.5)" },
] as const;

type ModelId = (typeof MODEL_OPTIONS)[number]["id"];

type InlineToken =
  | { type: "text"; text: string }
  | { type: "code"; text: string };

type MarkdownBlock =
  | { type: "heading"; text: string }
  | { type: "ordered-list"; items: InlineToken[][] }
  | { type: "paragraph"; tokens: InlineToken[] };

function tokenizeInline(text: string): InlineToken[] {
  return text
    .split(/(`[^`]+`)/g)
    .filter(Boolean)
    .map((part) => {
      if (part.startsWith("`") && part.endsWith("`")) {
        return { type: "code", text: part.slice(1, -1) };
      }
      return { type: "text", text: part };
    });
}

function parseMarkdown(markdown: string): MarkdownBlock[] {
  const lines = markdown.split(/\r?\n/);
  const blocks: MarkdownBlock[] = [];

  let index = 0;
  while (index < lines.length) {
    const line = lines[index].trim();

    if (!line) {
      index += 1;
      continue;
    }

    const headingMatch = line.match(/^#{1,3}\s+(.+)$/);
    if (headingMatch) {
      blocks.push({ type: "heading", text: headingMatch[1].trim() });
      index += 1;
      continue;
    }

    const orderedMatch = line.match(/^\d+\.\s+(.+)$/);
    if (orderedMatch) {
      const items: InlineToken[][] = [];
      while (index < lines.length) {
        const candidate = lines[index].trim();
        const nextMatch = candidate.match(/^\d+\.\s+(.+)$/);
        if (!nextMatch) break;
        items.push(tokenizeInline(nextMatch[1].trim()));
        index += 1;
      }
      blocks.push({ type: "ordered-list", items });
      continue;
    }

    const paragraphLines: string[] = [];
    while (index < lines.length) {
      const candidate = lines[index].trim();
      if (!candidate) break;
      if (/^#{1,3}\s+/.test(candidate)) break;
      if (/^\d+\.\s+/.test(candidate)) break;
      paragraphLines.push(candidate);
      index += 1;
    }

    blocks.push({
      type: "paragraph",
      tokens: tokenizeInline(paragraphLines.join(" ")),
    });
  }

  return blocks;
}

function InlineMarkdown({ tokens }: { tokens: InlineToken[] }) {
  return (
    <>
      {tokens.map((token, index) =>
        token.type === "code" ? (
          <code key={`${token.text}-${index}`} className="markdown-inline-code">
            {token.text}
          </code>
        ) : (
          <span key={`${token.text}-${index}`}>{token.text}</span>
        ),
      )}
    </>
  );
}

function MarkdownStructuredPreview({ markdown }: { markdown: string }) {
  const blocks = useMemo(() => parseMarkdown(markdown), [markdown]);

  if (blocks.length === 0) {
    return <div className="markdown-panel-empty">Markdown output is empty.</div>;
  }

  return (
    <div className="markdown-rendered">
      {blocks.map((block, index) => {
        if (block.type === "heading") {
          return (
            <h3 key={`${block.type}-${index}`} className="markdown-rendered-heading">
              <span className="markdown-rendered-heading-marker">#</span>
              {block.text}
            </h3>
          );
        }

        if (block.type === "ordered-list") {
          return (
            <ol key={`${block.type}-${index}`} className="markdown-rendered-list">
              {block.items.map((item, itemIndex) => (
                <li key={`${index}-${itemIndex}`}>
                  <InlineMarkdown tokens={item} />
                </li>
              ))}
            </ol>
          );
        }

        return (
          <p key={`${block.type}-${index}`} className="markdown-rendered-paragraph">
            <InlineMarkdown tokens={block.tokens} />
          </p>
        );
      })}
    </div>
  );
}

export function MarkdownPanel() {
  const {
    sessionId,
    output,
    editedSegments,
    editedEvents,
    isRegenerating,
    setOutput,
    setRegenerating,
  } = useEditorStore();
  const addToast = useToastStore((state) => state.addToast);
  const [model, setModel] = useState<ModelId>("local-qwen");

  const handleCopy = useCallback(async () => {
    if (!output) return;
    try {
      await navigator.clipboard.writeText(output.final_markdown);
      addToast({
        message: "Markdown copied to clipboard",
        type: "success",
        duration: 2000,
      });
    } catch {
      addToast({
        message: "Copy failed",
        type: "error",
        duration: 3000,
      });
    }
  }, [output, addToast]);

  const handleRegenerate = useCallback(async () => {
    if (!sessionId) return;
    setRegenerating(true);
    try {
      const next = await sessionRegenerate(
        sessionId,
        editedSegments,
        editedEvents,
      );
      setOutput(next);
      addToast({
        message: "Intent regenerated",
        type: "success",
        duration: 2000,
      });
    } catch (error) {
      addToast({
        message: error instanceof Error ? error.message : "Regenerate failed",
        type: "error",
        duration: 3000,
      });
      setRegenerating(false);
    }
  }, [
    sessionId,
    editedSegments,
    editedEvents,
    setOutput,
    setRegenerating,
    addToast,
  ]);

  return (
    <div className="markdown-panel">
      <header className="markdown-panel-header">
        <div className="markdown-panel-header-label">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
            <path d="M6 5h12" strokeLinecap="round" />
            <path d="M6 12h12" strokeLinecap="round" />
            <path d="M6 19h7" strokeLinecap="round" />
          </svg>
          Compiled Markdown
        </div>
        <h2 className="markdown-panel-title">Markdown Editor</h2>
      </header>

      <div className="markdown-panel-preview" aria-live="polite">
        <div className="markdown-panel-preview-chrome" aria-hidden>
          <span />
          <span />
          <span />
        </div>
        {output ? (
          <MarkdownStructuredPreview markdown={output.final_markdown} />
        ) : (
          <div className="markdown-panel-empty">
            No markdown yet. Once the current session is processed, the
            compiled result will render here.
          </div>
        )}
      </div>

      <div className="markdown-panel-actions">
        <button
          type="button"
          className="markdown-btn markdown-btn-primary"
          onClick={handleCopy}
          disabled={!output}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
            <rect x="9" y="9" width="11" height="11" rx="2" />
            <path d="M5 15V6a2 2 0 0 1 2-2h9" />
          </svg>
          Copy to Clipboard
        </button>
        <button
          type="button"
          className="markdown-btn markdown-btn-secondary"
          onClick={handleRegenerate}
          disabled={!sessionId || isRegenerating}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
            <path d="M21 12a9 9 0 1 1-3-6.7" />
            <path d="M21 4v5h-5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          {isRegenerating ? "Regenerating..." : "Regenerate Intent"}
        </button>

        <label className="markdown-panel-model">
          <span className="sr-only">Model</span>
          <select
            className="markdown-panel-model-select"
            value={model}
            onChange={(event) => setModel(event.target.value as ModelId)}
          >
            {MODEL_OPTIONS.map((option) => (
              <option key={option.id} value={option.id}>
                {option.label}
              </option>
            ))}
          </select>
          <svg
            className="markdown-panel-model-caret"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
            aria-hidden
          >
            <path d="M6 9l6 6 6-6" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </label>
      </div>
    </div>
  );
}
