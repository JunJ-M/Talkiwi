# Talkiwi PRD Discovery

- Date: 2026-04-12
- Status: Discovery
- Product: Talkiwi

## 1. Current Positioning

### One-line Positioning

Talkiwi is an open-source, multi-track, plugin-first voice input orchestration layer that compresses natural speech, user actions, and environmental context into AI-friendly structured input.

### Vision

Users should not need to learn prompt writing. They should be able to speak naturally and operate naturally, while Talkiwi automatically captures relevant context and assembles it into model-ready input.

## 2. Problem Statement

Current voice input products have four structural gaps:

1. Speech without context
2. Transcription without action evidence
3. Raw spoken language without AI-friendly restructuring
4. Closed context model without extensibility

## 3. Core Product Thesis

Talkiwi upgrades input from:

`microphone -> textbox`

to:

`speech + actions + artifacts + traces + plugins -> structured context package -> LLM / agent`

## 4. Core Concepts

### Track Model

1. Speech Track
2. Action Track
3. Artifact Track
4. Trace Track
5. Plugin Track

### Key Engine

The critical layer is an intent compiler that turns spoken language and surrounding context into:

- explicit task
- inferred intent
- relevant artifacts
- constraints
- missing context
- final prompt

## 5. Current Target Users

1. Heavy AI users switching across ChatGPT, Claude, Cursor, and Copilot
2. Developers speaking while browsing files, selecting code, pasting logs, and taking screenshots
3. PMs, researchers, and operators combining speech with pages, docs, screenshots, and snippets
4. Voice-first workflow users who want "speak + act" instead of manual prompt writing

## 6. Current MVP Scope Draft

### V1 Must-have

1. Push-to-talk / recording
2. ASR transcription
3. Selected text injection
4. Screenshot injection
5. Current URL / page title injection
6. File drag-in injection
7. AI-friendly prompt rewrite
8. Multi-track timeline debug panel
9. One-click send to ChatGPT / Claude / Cursor / custom API

### V1.5 Candidate

1. Plugin SDK
2. IDE / Terminal plugins
3. Track weights and ranking
4. Prompt templates
5. Local cache and session replay

### V2 Candidate

1. Ambient mode
2. Automatic scene recognition
3. Intelligent track recall
4. Long-session memory packing
5. Team collaboration and shared plugin marketplace

## 7. Non-functional Requirements

### Performance

- First-pass organization within 1 to 2 seconds after speech ends
- Multi-track merge within around 3 seconds
- Local-first and minimal upload

### Privacy

- Local processing by default
- Per-track permissioning
- Explicit permission for screenshots, documents, and clipboard
- Session-level non-recording mode

### Observability

- Users can inspect the final assembled input
- Users can inspect each track contribution
- Users can disable a track
- Users can replay why context was included

## 8. Success Metrics Draft

1. Prompt acceptance rate
2. Second-edit rate
3. Context hit rate
4. Task completion rate
5. Input time reduction

## 9. Product Risks and Open Issues

### Strategic Risks

1. The product may be valuable as infrastructure but too abstract as a standalone end-user app
2. Multi-track capture may create permission friction and trust issues
3. The "intent compiler" quality may dominate perceived product value
4. Cross-app context capture may be expensive to build and maintain across platforms

### Open Questions

1. What is the exact first platform: macOS desktop app, browser extension, IDE plugin, or hybrid?
2. What is the exact first sending target: ChatGPT, Claude desktop/web, Cursor, or custom APIs?
3. Is the first wedge "developer workflow" or a broader "all AI heavy users" category?
4. What should be the primary user-visible unit: a session, a prompt draft, or a task timeline?
5. How much automation should V1 do by default versus explicit user-triggered capture?
6. Should Talkiwi own the final chat UI, or only assemble and hand off prompts?
7. What is the first revenue or distribution hypothesis, if any?

## 9.1 Round 1 Decisions

### Current Wedge

The first real wedge is:

`AI heavy users, starting from developer workflows`

More specifically:

- users speak requirements naturally
- users simultaneously produce explicit actions such as selection, screenshot, and other traces
- Talkiwi merges the speech timeline and action timeline
- the local model / product model compiles them into an AI-friendly readable markdown package
- the user previews, edits, and sends that package onward

### Product Form

The first product form is:

- macOS desktop application
- desktop-widget-like interaction
- Talkiwi owns the input, preview, edit, and send experience

### V1 Context Policy

V1 is explicit by default:

- only user-explicit selections
- only user-explicit screenshots
- only user-explicit attachments
- no aggressive automatic context injection

This is a trust-first choice and should shape both UX and permissions.

### V1 Essential Tracks

The must-have tracks currently are:

1. Speech
2. Selected Text
3. Screenshot
4. Current URL / Page
5. Clipboard

### Output Form

The output should be:

- a readable AI-friendly markdown package
- usable across any downstream model surface
- not tightly coupled to a single target model platform in V1

### Primary Success Metric

The primary north-star metric is:

`Prompt acceptance rate`

## 10. Reflection

The concept is strong because it reframes voice input as context compilation rather than transcription. The current requirement draft is already rich, but the hardest product decisions are still unresolved:

1. narrow initial user wedge
2. narrow initial platform and permissions model
3. define the minimum context package that feels magical without feeling invasive
4. decide whether the product is primarily a user app, an agent input layer, or a developer platform

Without these decisions, the roadmap is directionally correct but still too broad for a sharp PRD.

After Round 1, the direction is sharper. The product is no longer "voice for everyone"; it is becoming a macOS-native context compiler for AI-heavy developer workflows. That is good.

However, the next ambiguity is operational rather than conceptual:

1. what exact interaction starts and ends a capture session
2. what exact markdown structure users preview before sending
3. how explicit artifacts are linked to spoken references like "this", "that error", or "the page I just opened"
4. what "send" means if V1 is intentionally target-agnostic
5. whether URL / page capture can remain explicit while still feeling seamless

These choices will define whether the product feels magical, trustworthy, and fast rather than just conceptually interesting.

## 11. Next Interview Topics

1. Who is the first user we are willing to disappoint less than everyone else?
2. Which specific workflow should feel 10x better in V1?
3. What context sources are mandatory versus nice-to-have?
4. What output handoff model is acceptable in the first release?
5. What privacy defaults are non-negotiable?
