# Starting work, templates, voice and storage

Research date: 2026-10-06. This describes what's built.

## Starting work

With no work open, or after Ctrl+Space then `space n`, the main area shows the welcome screen. Everything is chosen with ↑↓ and ⏎; nothing is typed in a command language.

```text
  ▸ Quick start       one agent in a folder · ⏎ ⏎ to go
    From a template   feat · fix · refactor · research · review · docs
    Build your own    coming soon

  RECENT
    1  fix login      fix      ~/code/app
```

- **Quick start:** folder, name and agent, all pre-filled. Pressing ⏎ ⏎ from the welcome screen starts it.
- **From a template:** pick a kind; the form is filled from it (team, permission mode, the brief's questions).
- **The form** is one page in three sections: WHERE (folder, name), TEAM (lead, children, what they run, permits) and BRIEF (template questions), then `start`.
  - Every field has a default, so ⏎ through it is enough.
  - Only the focused field is highlighted (an accent `┃` bar). Only it shows its description and its options; the others show just their value.
  - Mistakes show under the field (`✗ give the work a name`), and the help line lists the focused field's keys.
  - ↑↓ or Tab move between fields, ←→ change a choice, and digits type a number.
- **Children have no limit.** The form sets how many to start with. Add more while the work runs: Ctrl+Space then `space c` adds one, and `:add [preset] [n]` adds several, e.g. `:add codex 4`. Templates' `children` and `new … --children` have no cap either.
- **Forms are pluggable** (`makima_tui::form`). A form is groups of `Field`s read back by key.
  - Built-in fields: `TextInput`, `Select`, `Number`, `Action` (opens something, like the folder picker) and `Button`.
  - A new kind of input implements the `Field` trait; a new look is a `Theme`.
- **Folder picker:** ⏎ on the folder field opens a fuzzy-filtered list (nucleo-matcher). The work's name follows the folder until you edit it. Typing a path that starts with `/` or `~` uses it directly.
  - **Ranking:** folders you've worked in come first, ranked by frecency. Each work counts for its folder, weighted by age (100 within 4 days, down to 10 after 90), so a repo used daily beats one used once.
  - **Repos:** found by walking `[work] roots` (default `~`) `depth` folders down (default 5). The walk runs on a background thread and starts again each time the welcome screen opens. It looks inside repos too, so nested repos and submodules show up. It skips hidden folders, dependencies and build output, and `~/Library`, `Applications`, `Movies`, `Music` and `Pictures`. Folders macOS won't let it read are skipped, not fatal. On a typical home folder it takes about 30 ms.
  - **Why not fff:** we tried [fff](https://github.com/dmtrKovalenko/fff). It's fast, but it's a file index for one tree: home scanning is off by default, and it brings LMDB, libgit2 and a watcher in a nightly release. We need a list of repo folders, which a bounded walk gives directly.
- **Recent work:** `1`–`9` reopens a work. Its agents restart in the same folder, and Claude Code agents resume their sessions with `--resume <session-id>`.
- **Build your own:** visible but disabled for now.
- **Scripting:** the `makima>` prompt (Ctrl+Space then `:`: `new …`, `add …`) and launch flags (`--work`, `--preset`, `--children`) still work.

## Templates and kinds

| Kind | Means | Team | Brief asks |
|---|---|---|---|
| feat | add something new | lead + implementer + tester | what it should do, done when, not in scope, constraints |
| fix | something is broken | lead + tester | what's broken, how to reproduce, expected instead, where to look |
| refactor | restructure without changing behaviour | lead + reviewer | what, why, what must not change, done when |
| research | investigate; no code changes | lead + researcher, read-only | the question, where to look, what to deliver |
| review | assess a change or code | lead + 2 reviewers, read-only | what to review, focus, what to deliver |
| docs | write or update documentation | lead | what's missing, who reads it, where it goes |

The questions follow how specs for coding agents are written: a goal, checkable "done when", explicit non-goals and constraints. The first question must be answered before starting, because the agents start from it.

A kind only picks defaults; the form can change the team. Templates live in `makima-core` (`builtin_templates`). `[[templates]]` in the config adds new ones or replaces built-ins by id. `makima templates` lists them.

**What agents receive:**
- **Template work:** the answers and notes go into `brief.md` in the work's folder.
  - Every agent starts with an opening prompt that gives it its job.
  - **The team coordinates through `plan.md`** next to the brief, because the agents are separate sessions: the lead can't message its children, and they can't see its conversation. This follows [Anthropic's multi-agent research](https://www.anthropic.com/engineering/multi-agent-research-system) and agent-team practice.
    - The lead plans first. It writes a `## child N` section per child, each with a task, the files it owns (no two tasks share files) and a done-when. It leaves children idle if the work is small, then tells you the plan is ready.
    - Each child is told its number and role, replies only "ready" until told to go, and then works from its section. It touches only its files, and adds status lines (started, blocked, done) under its section.
    - Telling everyone to go is `space a` (message everyone).
  - Each built-in kind has its own lead prompt. For example, a fix lead is told to reproduce the bug first, find the root cause, make the smallest fix and have the tester prove it.
  - Children get their role and its duty, e.g. a tester writes and runs tests that show the problem, then that it's fixed.
  - Read-only templates add *"Do not modify files."*
  - Templates can replace both prompts with `lead-prompt` and `child-prompt`, using `{name}` `{kind}` `{folder}` `{brief}` `{plan}` `{team}` (repeats counted, e.g. `lead (claude) + 6 × implementer (claude)`), `{protocol}` (the plan.md rules above), plus `{number}`, `{role}` and `{duty}` for children (see `example.toml`).
- **Quick start:** no brief.
- **Permissions:** see below.
- **Sessions:** presets with a session flag (Claude Code: `--session-id`) start with a fresh UUID, saved per agent.
  - Reopening resumes a session only if Claude actually saved it (`~/.claude/projects/*/<id>.jsonl`, written after an agent's first message).
  - An agent that never got a message starts fresh with the same id, instead of failing with "No conversation found".

## Permissions

How much agents may do without asking. One vocabulary, which each preset turns into its program's flags:

| Mode | Claude Code | Codex |
|---|---|---|
| `manual` | `--permission-mode default` | `--sandbox workspace-write --ask-for-approval untrusted` |
| `accept-edits` | `--permission-mode acceptEdits` | `--sandbox workspace-write --ask-for-approval on-request` |
| `plan` | `--permission-mode plan` | `--sandbox read-only --ask-for-approval on-request` |
| `auto` | `--permission-mode auto` | `--sandbox workspace-write --ask-for-approval never` |
| `bypass` | `--dangerously-skip-permissions` | `--dangerously-bypass-approvals-and-sandbox` |

- **Config:** `[permissions] default = "auto"` applies to every agent, and `agents = { codex = "plan" }` sets one per preset. Unset leaves each program to its own default.
- **Other programs:** your own presets give their flags with `permission-flags = { auto = "--yes", bypass = "--yes-always" }`. Modes without flags add nothing.
- **Templates:** read-only templates (research, review) use `plan`. Any template can set `permission`.
- **The form:** the `permits` field defaults to the template's mode, or "from config". Changing it sets the mode for the whole team.
- **Children:** children added later follow the work's mode.
- **Bypass:** a pane running in bypass mode shows a red `⚠ bypass` in its border.
- **Saved:** the mode is stored per agent in `work.db`, so reopening resumes with the same flags.

## Voice

- **Ctrl+Space then `space v`** starts recording; pressing it again stops and transcribes. It works in any mode, including while typing, and returns to whatever you were doing.
- **In the new-work form**, the text goes into the focused brief question, or into a free note.
- **Mid-work**, it opens the talk line with the text in it, so you can check it and choose `@lead`, `@all` or `@2` before sending. The recording is saved with the message.
- **Recording:** `VoiceRecorder` writes AAC `.m4a` at 16 kHz mono to `~/.cache/makima/recordings`. When the work is saved, the file moves into the work's `audio/` folder with its SHA-256.
- **Transcription:** both providers sit behind a `Transcriber` protocol, and `VoiceInput` turns a recording into a clip through whichever the config picks.
  - **`AppleTranscriber`** uses macOS 26's on-device `SpeechAnalyzer`. It's private and free, and downloads its language model on first use.
  - **`APITranscriber`** posts the recording to any OpenAI-compatible `/audio/transcriptions` endpoint: OpenAI, Groq or a local Whisper server.
- **Config:**

  ```toml
  [voice]
  provider = "openai"                      # apple | openai | none
  model = "gpt-4o-transcribe"              # Groq: "whisper-large-v3"
  base-url = "https://api.openai.com/v1"   # Groq: "https://api.groq.com/openai/v1"
  api-key-env = "OPENAI_API_KEY"
  # language = "en-US"
  ```

- **API keys** never go in the config, and `api-key = …` is rejected. Makima reads the environment variable named by `api-key-env`. Apps opened from the Dock don't inherit your shell's environment, so it falls back to the Keychain: `security add-generic-password -s makima -a openai -w`. A missing key is reported when you start recording, not after.
- **Info.plist:** it's embedded in the executable and provides a bundle id plus the microphone and speech usage descriptions that macOS requires.
- **Test:** `MAKIMA_VOICE_TESTS=1 just test` transcribes real speech generated by `say -v Samantha`.

## Storage

Paths follow XDG on macOS too, via the `etcetera` crate (`makima paths` prints them):

```text
~/.config/makima/config.toml          settings
~/.local/share/makima/                durable data, or [storage] root
  index.db                            every work, for recents
  work/<uuidv7>-<slug>/
    work.db                           this work's SQLite database (WAL)
    brief.md
    audio/<id>.m4a
~/.local/state/makima/                state
~/.cache/makima/recordings/           voice notes not yet saved with a work
```

`work.db` (`makima-store`, `rusqlite` with SQLite bundled, migrations through `rusqlite_migration`):

| Table | Holds |
|---|---|
| `work` | id, name, kind, template, folder, read-only, created |
| `agents` | position (0 is the lead), name, role, preset, command, Claude session id, start/end, exit code |
| `notes` | brief answers and free notes, linked to their recording |
| `messages`, `message_recipients` | everything sent to agents, and who received it |
| `audio`, `transcripts` | recordings with hashes and durations; provider, language and text |
| `events` | append-only journal: work created or reopened, agents started or exited, status changes |

Claude Code's own transcripts stay in `~/.claude/projects/…/<session-id>.jsonl`. `agents.session_id` links to them rather than copying them. `makima works` lists saved work.
