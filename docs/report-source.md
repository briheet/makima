# Makima work-package and coordination architecture

Research date: 2026-09-08

## Executive decision

Makima should use a user-selected, ordinary filesystem directory as its durable "Makima Vault." The vault is both the human workspace and the portable system of record. Obsidian can open that same directory directly because a vault is just a folder of plain-text Markdown files, and Obsidian detects external file changes. Keep the Makima vault separate from every source repository because one work item can span several repositories and because archived work should survive repository moves or deletion. Do not build the primary store around symlinks: Obsidian explicitly warns that symlinks can cause sync corruption, duplicated files, or crashes.

The v1 data model should be:

- one folder per work item;
- one human-facing `README.md` with flat YAML frontmatter;
- one structured `repositories.yaml` for multi-repository bindings;
- ordinary attachment files inside the work folder;
- one voice-session folder per recording, containing audio, transcript, and metadata;
- one immutable-ish run folder per launch, containing its resolved context, agent plan, journal, artifacts, and optional terminal captures;
- a machine-local repository-resolution database outside the portable vault.

This follows Obsidian's strengths rather than fighting them. Obsidian stores notes as Markdown and properties in YAML frontmatter; its property UI handles a flat set of primitive values and does not support nested properties well. Bases can create table, list, card, and kanban-like views from those Markdown properties, and `.base` files are YAML. [How Obsidian stores data](https://obsidian.md/help/Files%2Band%2Bfolders/How%2BObsidian%2Bstores%2Bdata), [Properties](https://obsidian.md/help/Editing%2Band%2Bformatting/Properties), [Bases](https://obsidian.md/help/bases), [Bases syntax](https://obsidian.md/help/bases/syntax), [Symlink warning](https://obsidian.md/help/Files%2Band%2Bfolders/Symbolic%2Blinks%2Band%2Bjunctions)

## Filesystem layout

```text
Makima/
  Home.md
  Work.base
  Repositories.base
  context/
    product/
    people/
    standards/
  repositories/
    <repository-uuid>.md
  work/
    <work-uuid>-<slug>/
      README.md
      repositories.yaml
      context/
      attachments/
      conversation.md
      voice/
        <voice-session-uuid>/
          audio.ogg
          transcript.vtt
          metadata.json
      runs/
        <run-uuid>/
          run.json
          plan.md
          events.jsonl
          agents/
            <agent-uuid>.cast
          artifacts/
          summary.md
```

Use UUIDv7 for work, run, repository, message, resource, voice-session, and agent-instance IDs. UUIDv7 is time ordered while retaining random bits, which makes filesystem and event ordering convenient without putting mutable meaning into identifiers. Use UTC RFC 3339 timestamps everywhere. [UUIDv7, RFC 9562](https://www.rfc-editor.org/info/rfc9562/), [RFC 3339](https://www.rfc-editor.org/info/rfc3339/)

### Work note

Keep searchable/filterable fields in flat frontmatter and nuanced context in Markdown:

```markdown
---
schema: makima.work/v1
id: 0199...
title: Add repository-aware work creation
kind: feat
status: clarifying
priority: normal
created: 2026-09-08T06:30:00Z
updated: 2026-09-08T06:45:00Z
tags:
  - makima/work
  - desktop
---

# Add repository-aware work creation

## Intent

## Desired outcome

## Acceptance criteria

## Constraints

## Context

## Decisions

## Open questions
```

Task kinds should be configurable, with a useful built-in vocabulary: `feat`, `fix`, `chore`, `research`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `security`, `migration`, `ops`, and `review`. Conventional Commits establishes `feat` and `fix` and permits other project-defined types such as build, chore, CI, docs, refactor, performance, and test; Makima should borrow the vocabulary without making task semantics depend on commits. [Conventional Commits](https://www.conventionalcommits.org/)

Use standard relative Markdown links for portability; allow Obsidian embeds for convenience. Obsidian supports images, audio, video, and PDF attachments as ordinary files and can embed them. [Internal links](https://obsidian.md/help/Linking%2Bnotes%2Band%2Bfiles/Internal%2Blinks), [Attachments](https://obsidian.md/help/attachments), [Accepted formats](https://obsidian.md/help/file-formats), [Embeds](https://obsidian.md/help/embeds)

### Repository bindings

Do not put nested repository structures into frontmatter. Use `repositories.yaml`, parsed into a JSON-compatible data model and validated by a shipped JSON Schema 2020-12 schema:

```yaml
schema: makima.repositories/v1
repositories:
  - id: 0199-repository-uuid
    role: primary
    base: main
    access: write
    scopes:
      - apps/desktop
      - apps/orchestrator
  - id: 0198-library-repository-uuid
    role: dependency
    ref: 47ac2e1
    access: read
    scopes:
      - include
      - src/apprt
```

`id` is the stable portable identity. `role` explains why the repository participates. `base` or `ref` is the requested starting point. `access` is `read`, `write`, or `none`. `scopes` narrows the paths agents may use. Optional fields can include `depends_on`, `required_checks`, and `worktree_policy`. JSON Schema Draft 2020-12 is the appropriate current schema language for validating the parsed manifest. [JSON Schema specification](https://json-schema.org/specification)

Each `repositories/<uuid>.md` stores portable identity and shared context: display name, canonical remote URLs, default branch, tags, and notes. It must not store the only copy of an absolute checkout path.

The desktop app maintains a local repository-resolution store outside the vault:

```text
repository UUID -> local path + security-scoped bookmark + last verified remotes
```

On macOS, let the user select a repository or parent folder with `NSOpenPanel`; persist access with security-scoped bookmarks when sandboxed. Apple documents that a bookmark can preserve access across launches and must be resolved and explicitly accessed. [Apple: accessing files from the macOS App Sandbox](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox)

When adding a repository, Makima should:

1. resolve the Git top level;
2. capture canonical remotes, current/default branch, and a repository UUID;
3. save the portable definition in the vault and the local path/bookmark in device-local state;
4. on every launch, verify that the resolved folder still matches expected Git metadata;
5. ask the user to relink if it moved or no longer matches.

At run start, resolve every binding and snapshot `path`, HEAD SHA, branch, dirty state, and a hash of any included working-tree diff. The task stays portable while the run remains reproducible.

### Repository isolation

Use Git worktrees for write-capable agent work. Git officially supports multiple working trees and different branches checked out concurrently; it refuses to check out an already checked-out branch in another worktree by default. Create a distinct task/agent branch for every simultaneous writer. [git-worktree](https://git-scm.com/docs/git-worktree)

The safe default is one writer per repository within a work item. Parallel researchers and reviewers may use read-only access. If two agents must write the same repository, give each a distinct worktree and branch and assign an explicit integrator. A shared writable checkout is not coordination; it is a race condition.

## Context assembly

Makima should not concatenate an entire repository into every prompt. Assemble context in layers:

1. global vault context;
2. repository notes and repository-native instructions;
3. work `README.md` and `context/` notes;
4. explicitly selected attachments;
5. an immutable run snapshot of repository revisions and resource hashes;
6. agent-specific role, task, and scratch context.

Build repository inventories from Git tracked/non-ignored paths and let agents retrieve files on demand. `.gitignore` and Git inventories are discovery aids, not a security boundary. Add explicit size, binary, secret, and scope policies before a resource can enter agent context. [gitignore](https://git-scm.com/docs/gitignore), [git-ls-files](https://git-scm.com/docs/git-ls-files)

Expose the assembled context to agents through a small Makima MCP server:

```text
makima://work/<work-id>/brief
makima://work/<work-id>/conversation
makima://work/<work-id>/attachment/<resource-id>
makima://repo/<repo-id>/file/<path>?rev=<sha>
```

MCP is designed for URI-addressed resources, tools, prompts, roots, progress, cancellation, and logging, and its content model includes images, audio, resource links, and embedded resources. This makes it the context/tool plane, not the agent lifecycle plane. [MCP specification](https://modelcontextprotocol.io/specification/2025-06-18/index), [MCP resources](https://modelcontextprotocol.io/specification/2024-11-05/server/resources), [MCP tools](https://modelcontextprotocol.io/specification/draft/server/tools)

Freeze the base context revision when a run starts. If Obsidian or Makima changes a source file while agents run, emit `context.updated` and let the user or lead agent accept a refresh. Never silently mutate the meaning of an active prompt.

## Attachments and screenshots

Attachments are first-class resources, not miscellaneous files. Preserve originals under `attachments/`, prefix filenames with a resource UUID, and track the following in memory and the event journal: path, MIME type, size, SHA-256, caption, source, creation time, and whether it is included in agent context. Use registered MIME types rather than guessing from extensions. [IANA media types](https://www.iana.org/assignments/media-types/media-types.xhtml), [Named Information hashes, RFC 6920](https://www.rfc-editor.org/info/rfc6920/)

Generate thumbnails, OCR, and captions in a disposable local cache; never overwrite the original. Only explicitly referenced or selected attachments should be sent to an agent. Negotiate the agent's prompt capabilities first: ACP content supports text, images, audio, resource links, and embedded resources, with image/audio/embedded-context capabilities advertised separately. [ACP schema](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/schema/v1/schema.json)

If a vault is kept in Git, use Git LFS for large audio/video/binary files or exclude raw recordings by policy; Git LFS stores small pointer files in Git and the actual large content separately. Obsidian Sync plans also impose per-file limits, so retention and sync behavior must be visible settings. [Git LFS](https://git-lfs.com/), [GitHub: Git LFS storage model](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-git-large-file-storage), [Obsidian Sync plans](https://obsidian.md/help/sync/plans)

## Voice and real-time conversation

Voice should feed the same message bus as typing. It should not be a parallel agent protocol.

Use one folder per voice session:

```text
voice/<voice-session-uuid>/
  audio.ogg
  transcript.vtt
  metadata.json
```

Store audio as Ogg Opus (`audio/ogg; codecs=opus`). Opus in Ogg is suitable for low-latency speech and streaming while remaining compact, and `.ogg` is a format Obsidian can embed directly. Keep `transcript.vtt` because WebVTT provides media-relative timestamps and speaker/voice spans. Maintain `conversation.md` as the pleasant Obsidian-readable projection of finalized human and agent messages. Do not use one eternal transcription file: timestamps belong to a particular recording, corrections become ambiguous, and concurrent recording becomes impossible. [Ogg Opus, RFC 7845](https://www.rfc-editor.org/info/rfc7845/), [WebVTT](https://www.w3.org/TR/webvtt1/), [Obsidian accepted formats](https://obsidian.md/help/file-formats)

The voice path should be:

1. Zig captures PCM frames and owns the high-rate audio buffers.
2. A `VoiceController` sends frames to a selected transcription provider.
3. Partial/volatile transcript results update the UI only.
4. On push-to-talk release or VAD end, finalize the segment, encode `audio.ogg`, update `transcript.vtt`, and register the media resource.
5. Emit one durable multimodal `Message` containing finalized text plus an audio reference.
6. Route it to the selected agent, role/group, or work room.
7. If an agent is busy, queue the message; interrupt only through an explicit user action and an adapter-supported cancel/steer operation.

Apple's current Speech framework has `SpeechAnalyzer`/`SpeechTranscriber`, asynchronous result sequences, a volatile result range, and explicit finalization methods. That makes it the natural local-first macOS provider. OpenAI Whisper is a useful local cross-platform provider, but its model card notes that it is not real-time out of the box and can hallucinate; preserve original audio so a transcript can be corrected or regenerated. OpenAI Realtime can be an opt-in remote provider for low-latency speech sessions and VAD/interruption, but its transcription is asynchronous guidance rather than necessarily the exact audio interpretation. [Apple SpeechAnalyzer](https://developer.apple.com/documentation/speech/speechanalyzer), [Apple finalization](https://developer.apple.com/documentation/speech/speechanalyzer/finalize%28through%3A%29), [OpenAI Whisper](https://github.com/openai/whisper), [OpenAI Realtime API](https://platform.openai.com/docs/api-reference/realtime?lang=javascript)

Recommended defaults: push-to-talk, a prominent recording indicator, local transcription, no transmission of raw audio to an agent unless needed, and an explicit retention choice (`discard after transcript`, `retain with task`, `never record audio`). macOS requires a microphone purpose string and authorization before capture. [Apple microphone usage description](https://developer.apple.com/documentation/BundleResources/Information-Property-List/NSMicrophoneUsageDescription), [Apple media-capture authorization](https://developer.apple.com/documentation/bundleresources/requesting-authorization-for-media-capture-on-macos)

## Agent protocols and routing

Use ACP as the primary local agent adapter. Its JSON-RPC lifecycle provides initialization/capability negotiation, session creation/resumption, prompts, streamed updates, permission requests, cancellation, plans, tool calls, and terminal updates. Official adapters already exist for Codex and Claude, including images, context resources, workspace directories, permissions, and terminals. The embedded terminal is a first-class observability/manual-control surface, but Makima should not scrape terminal text to infer semantic state when ACP is available. [ACP v2 overview](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v2/overview.mdx), [Codex ACP adapter](https://github.com/agentclientprotocol/codex-acp), [Claude ACP adapter](https://github.com/agentclientprotocol/claude-agent-acp)

Use MCP behind those sessions to supply Makima context and tools.

Borrow the A2A data model internally: messages are communication and clarification, artifacts are results, tasks can enter `input-required`, and updates can stream. Do not implement A2A transport in local v1; add it later for remote independently hosted agents. The A2A specification explicitly says streamed messages are not a reliable channel for critical information and separates communication from task outputs, which is why Makima still needs its own durable journal. [A2A specification](https://a2a-protocol.org/latest/specification/)

For agents with no ACP adapter, provide a PTY adapter with reduced capability: visible terminal, input/output, process lifecycle, and optional explicit status hooks. Label inferred state as inferred. An optional asciicast recording can preserve terminal output and resize/exit timing; keep input recording off by default to avoid capturing secrets. [asciicast v3](https://docs.asciinema.org/manual/asciicast/v3/)

## Durable events and projections

Every run needs a single-writer append-only `events.jsonl`. Use a CloudEvents-compatible JSON envelope:

```json
{"specversion":"1.0","id":"0199...","source":"makima://run/0199...","type":"dev.makima.message.created.v1","subject":"work/0199...","time":"2026-09-08T07:00:00Z","datacontenttype":"application/json","data":{"message_id":"0199..."}}
```

CloudEvents requires `id`, `source`, `specversion`, and `type`; `source` plus `id` is the deduplication identity. Its optional `time` uses RFC 3339. [CloudEvents specification](https://github.com/cloudevents/spec/blob/main/cloudevents/spec.md), [CloudEvents JSON format](https://github.com/cloudevents/spec/blob/main/cloudevents/formats/json-format.md)

Persist before publishing these critical events: finalized user/agent messages, context revisions, work/run/agent state, permissions and decisions, repository snapshots, artifact/resource registration, and failures. Keep token chunks, partial transcripts, terminal redraws, audio frames, and presence indicators ephemeral by default. One journal writer per run prevents interleaved lines. Build `conversation.md`, `summary.md`, and UI state as replayable projections.

On restart, scan the work folders, parse the current Markdown/YAML, load `run.json`, replay the journal, and attempt ACP session resume. Any process that cannot be proved alive becomes `interrupted`; it must never silently reappear as `working`.

## In-memory control plane

The Elixir process hierarchy should mirror failure and ownership boundaries:

```text
Makima.Application
├── RepoRegistry
├── VaultSupervisor
│   ├── VaultIndex
│   └── WorkDynamicSupervisor
│       └── WorkSupervisor(work_id)
│           ├── WorkStore
│           ├── JournalWriter
│           ├── ConversationProjector
│           ├── ContextIndex / MCP server
│           └── RunDynamicSupervisor
│               └── RunSupervisor(run_id)
│                   ├── RunStore
│                   ├── RepoSandboxSupervisor
│                   └── AgentDynamicSupervisor
│                       └── AgentSession(adapter: ACP | PTY | A2A)
└── PubSub / EventRegistry
```

`WorkStore` is the authoritative in-memory projection for one work package:

```text
id, path, metadata, repositories, context_resources, attachments,
voice_sessions, conversation, active_run_id, runs, parse_errors,
content_hash, subscribers
```

`RunStore` holds:

```text
id, work_id, context_revision, repo_snapshots, plan, agents, status,
pending_permissions, message_queue, artifacts, started_at, updated_at
```

`AgentSession` holds:

```text
id, role, adapter, protocol_session_id, status, capabilities,
repo_access, inbox, current_turn, last_sequence
```

The normalized message is multimodal:

```text
Message {
  id, work_id, run_id?, sender, recipients, created_at,
  modality, parts[], reply_to?, durable
}
```

Recipients can be a specific agent, a role/group, or the work room. A finalized voice utterance and a typed prompt differ only in their content parts. Voice audio bytes should remain in Zig/native buffers and files; Elixir receives transcript partial/final events and resource references, not every PCM frame.

Watch the vault with a native filesystem watcher, debounce/coalesce changes, hash before reparsing, and periodically do a complete reconciliation. The Elixir `file_system` library uses native FSEvents/inotify/Windows watchers and publishes filesystem events to subscribers. Use atomic temp-file-and-rename writes. Before changing human-edited Markdown, compare its content hash; if Obsidian changed it, reparse/merge or surface a conflict. [Elixir file_system](https://file-system.hexdocs.pm/readme.html)

Only one orchestrator instance should hold the execution lease for a work package. Other synced devices may browse/edit, but two machines must not independently launch the same run.

## Work and agent lifecycles

Work-item lifecycle:

```text
draft -> clarifying -> ready -> launching -> active -> review -> archived
            |                       |          |
          blocked                 failed     active (rework)

cancelled can be reached from any non-archived state.
```

Agent execution lifecycle:

```text
starting -> idle -> working -> input_required -> working
                       |-> permission_required -> working
                       |-> completed
                       |-> failed
                       |-> cancelled
```

The user archives the work after accepting it. An agent reporting completion only moves the work to `review`.

Clarification is a real phase, not a chat prelude:

1. Start a read-only ACP session with the brief, repository inventory, and context.
2. Ask questions and record answers as durable messages.
3. Produce a proposed patch to the work note and acceptance criteria.
4. User accepts the clarification diff.
5. Produce a visible launch plan with roles, repository access, write ownership, dependencies, checks, and completion conditions.
6. Launch only after the plan resolves repository paths and capabilities.

Do not map `feat` to a fixed agent team. Kind helps filtering and templates; scope, uncertainty, repository count, and risk determine topology.

## Archive semantics and packaging

Archive by setting `status: archived`, not by moving the folder. Stable paths preserve relative links and avoid inconsistencies when Obsidian or a sync service observes the move. `Work.base` can provide Active and Archived views.

Before archive: stop or detach all agent sessions, finalize voice transcripts, render the final summary/conversation, record final repository revisions/artifacts, and generate a SHA-256 manifest. BagIt standardizes directory payloads plus checksum manifests; RO-Crate standardizes a directory-shaped research object with machine-readable metadata and provenance. Neither should be mandatory for a live v1 task, but borrowing a checksum manifest now and adding optional BagIt/RO-Crate export later gives Makima a clean preservation path. [BagIt, RFC 8493](https://www.rfc-editor.org/info/rfc8493/), [RO-Crate 1.3](https://www.researchobject.org/ro-crate/specification/1.3/introduction.html)

## Initial UI flow

Use a configurable command palette; default to `Cmd+K` on macOS. Bare `Shift+:` produces `:` and conflicts with terminal/Vim input, so it should not be the default global command unless the terminal is unfocused.

Palette commands:

- Add Repository
- New Work
- Attach Screenshot or File
- Record Voice Note
- Clarify Work
- Generate Launch Plan
- Start Work
- Talk to Agent / Group / Work Room
- Review Work
- Archive Work

`New Work` should be a short wizard: choose kind, title, repositories, per-repository access/scope, and optional attachments/voice note. It creates `draft`, then opens the full work note. `Clarify` is the first agent action. `Start` should be unavailable until required repositories resolve, the context is valid, and a launch plan exists.

## Recommended implementation order

1. Vault selection, work-folder parser/writer, flat frontmatter, `Work.base`, and file watching.
2. Portable repository registry plus device-local path/bookmark resolution.
3. In-memory WorkStore/RunStore/message/event schemas and journal replay.
4. Work creation and clarification UI with attachment capture.
5. One ACP adapter and read-only context delivery through the Makima MCP server.
6. Single-repository worktree launch, permissions, review, and archive.
7. VoiceController with push-to-talk, Apple Speech provider, Ogg Opus storage, VTT, and normalized messages.
8. Multi-repository launch plans and writer isolation.
9. PTY fallback/terminal capture, additional ACP agents, remote A2A, optional export formats.

This order proves the durable model before adding multi-agent concurrency. Voice becomes a new input modality on an existing message system rather than forcing a redesign later.

## Decisions to lock for v1

- Makima vault is separate from source repositories.
- Task/work item is one folder and the durable unit of coordination.
- Markdown is the human truth; structured sidecars hold nested/machine-owned data.
- Absolute repository paths and security bookmarks are local-only.
- UUIDv7 and RFC 3339 are universal identifiers/timestamps.
- ACP controls local agents; MCP supplies context/tools; A2A is deferred remote interoperability.
- One writer per repository by default; all concurrent writers get isolated worktrees/branches.
- `conversation.md` is a projection; `events.jsonl` is the durable causal record.
- Partial transcripts and token streams are ephemeral; finalized messages are durable.
- Each voice session owns `audio.ogg`, `transcript.vtt`, and `metadata.json`.
- Archive is a status transition, not a folder move.

