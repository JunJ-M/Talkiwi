import { useMemo } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useEditorStore } from "../../stores/editorStore";
import type { ActionEvent, SpeakSegment } from "../../types";

function formatOffset(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `[${minutes.toString().padStart(2, "0")}:${seconds
    .toString()
    .padStart(2, "0")}]`;
}

function getActionTitle(event: ActionEvent): string {
  switch (event.action_type) {
    case "screenshot":
      return "Screenshot";
    case "selection.text":
      return "Selected text";
    case "clipboard.change":
      return "Clipboard";
    case "page.current":
      return "Active page";
    case "click.link":
      return "Link";
    case "file.attach":
      return "Attached file";
    case "window.focus":
      return "Window focus";
    case "click.mouse":
      return "Mouse click";
    default:
      return typeof event.action_type === "string"
        ? event.action_type
        : "Action";
  }
}

function getActionDetail(event: ActionEvent): string {
  const payload = event.payload as Record<string, unknown>;
  switch (event.action_type) {
    case "selection.text":
      return ((payload.text as string) ?? "").trim();
    case "page.current":
      return ((payload.title as string) || (payload.url as string) || "").trim();
    case "click.link":
      return ((payload.title as string) || (payload.to_url as string) || "").trim();
    case "clipboard.change":
      return (
        ((payload.text as string | undefined)?.trim() ??
          (payload.file_path as string | undefined) ??
          "(non-text payload)") as string
      );
    case "file.attach":
      return (
        ((payload.file_name as string | undefined) ||
          (payload.file_path as string | undefined) ||
          "") as string
      ).trim();
    case "window.focus":
      return (
        ((payload.window_title as string) ||
          (payload.app_name as string) ||
          "") as string
      ).trim();
    case "screenshot": {
      const ocrText = payload.ocr_text as string | undefined;
      return ocrText?.trim() || "Captured screen";
    }
    case "click.mouse":
      return `${(payload.button as string) ?? "mouse"} click`;
    default:
      return (event.semantic_hint ?? "").trim();
  }
}

type ActionCardVariant = "code" | "image" | "link" | "generic";

function getActionVariant(event: ActionEvent): ActionCardVariant {
  switch (event.action_type) {
    case "selection.text":
    case "clipboard.change":
    case "file.attach":
      return "code";
    case "screenshot":
      return "image";
    case "page.current":
    case "click.link":
      return "link";
    default:
      return "generic";
  }
}

function getActionImageUrl(event: ActionEvent): string | null {
  if (event.action_type !== "screenshot") return null;
  const payload = event.payload as Record<string, unknown>;
  const imagePath = payload.image_path as string | undefined;
  return imagePath ? convertFileSrc(imagePath) : null;
}

function getActionHost(event: ActionEvent): string {
  const payload = event.payload as Record<string, unknown>;
  const candidate =
    (payload.to_url as string | undefined) ||
    (payload.url as string | undefined) ||
    "";

  if (!candidate) return "talkiwi.local";

  try {
    return new URL(candidate).host;
  } catch {
    return candidate.replace(/^https?:\/\//, "");
  }
}

function getActionSnippet(event: ActionEvent): string {
  return getActionDetail(event)
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 180);
}

function buildBubbles(segments: SpeakSegment[]) {
  if (segments.length === 0) return [];

  const finals = segments.filter((segment) => segment.is_final);
  const source = finals.length > 0 ? finals : segments;

  return source.map((segment, index) => ({
    id: `${segment.start_ms}-${segment.end_ms}-${index}`,
    offset: segment.start_ms,
    text: segment.text.trim() || "(empty segment)",
  }));
}

function getAudioFileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? "audio.wav";
}

function actionIcon(event: ActionEvent): JSX.Element {
  switch (getActionVariant(event)) {
    case "code":
      return (
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
          <path d="M10 20l4-16" strokeLinecap="round" />
          <path d="M7 8l-4 4 4 4" strokeLinecap="round" strokeLinejoin="round" />
          <path d="M17 8l4 4-4 4" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      );
    case "image":
      return (
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
          <rect x="4" y="5" width="16" height="14" rx="2" />
          <circle cx="9" cy="10" r="1.6" />
          <path d="M20 15l-4.2-4.2a1.8 1.8 0 0 0-2.55 0L8 16" strokeLinecap="round" />
        </svg>
      );
    case "link":
      return (
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
          <path d="M10 14a5 5 0 0 0 7 0l2-2a5 5 0 1 0-7-7l-1 1" />
          <path d="M14 10a5 5 0 0 0-7 0l-2 2a5 5 0 0 0 7 7l1-1" />
        </svg>
      );
    default:
      return (
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
          <path d="M12 7v5l3 3" strokeLinecap="round" strokeLinejoin="round" />
          <circle cx="12" cy="12" r="8.5" />
        </svg>
      );
  }
}

function ActionCard({ event }: { event: ActionEvent }) {
  const variant = getActionVariant(event);
  const title = getActionTitle(event);
  const detail = getActionDetail(event) || "No additional context";
  const time = formatOffset(event.session_offset_ms);

  if (variant === "code") {
    return (
      <article className="review-action-card review-action-card-code">
        <header className="review-action-card-top">
          <span className="review-action-card-badge" aria-hidden>
            {actionIcon(event)}
          </span>
          <div className="review-window-dots" aria-hidden>
            <span />
            <span />
            <span />
          </div>
        </header>
        <div className="review-code-preview">
          <pre>
            <code>{getActionSnippet(event) || title}</code>
          </pre>
        </div>
        <footer className="review-action-card-footer">
          <div>
            <h4>{title}</h4>
            <p>{time}</p>
          </div>
        </footer>
      </article>
    );
  }

  if (variant === "image") {
    const imageUrl = getActionImageUrl(event);
    return (
      <article className="review-action-card review-action-card-image">
        <header className="review-action-card-top">
          <span className="review-action-card-badge" aria-hidden>
            {actionIcon(event)}
          </span>
          <div className="review-window-dots" aria-hidden>
            <span />
            <span />
            <span />
          </div>
        </header>
        <div className="review-image-preview">
          <div
            className="review-image-preview-frame"
            style={
              imageUrl
                ? { backgroundImage: `linear-gradient(180deg, rgba(15, 20, 34, 0.08), rgba(15, 20, 34, 0.32)), url("${imageUrl}")` }
                : undefined
            }
            aria-label={title}
          >
            {!imageUrl && <span>{detail}</span>}
          </div>
        </div>
        <footer className="review-action-card-footer">
          <div>
            <h4>{title}</h4>
            <p>{detail}</p>
          </div>
          <span className="review-action-time">{time}</span>
        </footer>
      </article>
    );
  }

  if (variant === "link") {
    return (
      <article className="review-action-card review-action-card-link">
        <header className="review-action-card-top">
          <span className="review-action-card-badge" aria-hidden>
            {actionIcon(event)}
          </span>
        </header>
        <div className="review-link-graphic" aria-hidden>
          <div className="review-link-graphic-wave" />
        </div>
        <footer className="review-action-card-footer">
          <div>
            <h4>{detail || title}</h4>
            <p>{getActionHost(event)}</p>
          </div>
          <span className="review-action-time">{time}</span>
        </footer>
      </article>
    );
  }

  return (
    <article className="review-action-card review-action-card-generic">
      <header className="review-action-card-top">
        <span className="review-action-card-badge" aria-hidden>
          {actionIcon(event)}
        </span>
        <span className="review-action-time">{time}</span>
      </header>
      <div className="review-action-card-body">
        <h4>{title}</h4>
        <p>{detail}</p>
      </div>
    </article>
  );
}

export function ReviewPanel() {
  const { sessionId, audioPath, editedSegments, editedEvents } =
    useEditorStore();

  const bubbles = useMemo(() => buildBubbles(editedSegments), [editedSegments]);
  const actionCards = useMemo(
    () =>
      editedEvents
        .slice()
        .sort((a, b) => a.session_offset_ms - b.session_offset_ms),
    [editedEvents],
  );
  const audioFallbackName = audioPath ? getAudioFileName(audioPath) : null;

  if (!sessionId) {
    return (
      <div className="review-empty">
        <div className="review-empty-glow" />
        <h2>Waiting for a session</h2>
        <p>
          Press record on the ball widget. Once a session completes, the
          detailed review and compiled markdown will appear here.
        </p>
      </div>
    );
  }

  return (
    <div className="review">
      <header className="review-header">
        <div className="review-header-label">
          <span className="review-header-label-icon" aria-hidden>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
              <path d="M9 12l2 2 4-4" strokeLinecap="round" strokeLinejoin="round" />
              <rect x="3" y="4" width="18" height="16" rx="3" />
            </svg>
          </span>
          <span>Timeline Analysis</span>
        </div>
        <div className="review-header-status">
          <span className="review-header-dot" aria-hidden />
          Auto-sync Active
        </div>
      </header>

      <div className="review-title-wrap">
        <h2 className="review-title">Detailed Review</h2>
      </div>

      <div className="review-stage-scroll">
        <div className="review-stage">
          <div className="review-stage-line" aria-hidden />
          <div className="review-stage-scrubber" aria-hidden />
          <div className="review-stage-divider" aria-hidden />

          <section className="review-lane review-lane-speak" aria-label="Speak">
            <div className="review-lane-label">Speak</div>
            <div className="review-card-row review-card-row-speak">
              {bubbles.length === 0 && (
                audioFallbackName ? (
                  <article className="review-speech-card review-speech-card-audio">
                    <p className="review-speech-card-text">
                      <span className="review-speech-card-time">[audio]</span>
                      Captured audio: {audioFallbackName}. Transcription is not
                      available for this session.
                    </p>
                  </article>
                ) : (
                  <div className="review-empty-inline">
                    No transcribed speech yet for this session.
                  </div>
                )
              )}
              {bubbles.map((bubble) => (
                <article key={bubble.id} className="review-speech-card">
                  <p className="review-speech-card-text">
                    <span className="review-speech-card-time">
                      {formatOffset(bubble.offset)}
                    </span>
                    {bubble.text}
                  </p>
                </article>
              ))}
            </div>
          </section>

          <section className="review-lane review-lane-action" aria-label="Action">
            <div className="review-lane-label">Action</div>
            <div className="review-card-row review-card-row-action">
              {actionCards.length === 0 && (
                <div className="review-empty-inline">
                  No captured actions in this session.
                </div>
              )}
              {actionCards.map((event) => (
                <ActionCard key={event.id} event={event} />
              ))}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
