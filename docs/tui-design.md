# Makima TUI design

Research date: 2026-10-06

## Status

- **Phases 1 and 2 are built:**
  - the chrome is a ratatui TUI in `crates/makima-tui`, drawn into a libghostty in-memory terminal by `TUIViewController`;
  - lead/children layout, zoom, Helix-style keys (or tmux-style, by config), a work picker, talk line with `@` routing, confirmations, the `makima>` prompt and the help overlay.
- **Phase 3 (the `tmux` shim for Claude Code agent teams) is not started.**
- **Starting work, templates, voice and storage are built;** see `work-and-voice.md`. They replace the `makima>` splash with a welcome screen. `space n` (Helix) or leader `n` (tmux) opens it, and `space v` is push-to-talk.
- **Keys** are below. The help overlay and the which-key popup are generated from the same tables as the bindings (`crates/makima-tui/src/keys.rs`), so they're always accurate.
- **Scripting and self-tests:** `MakimaApp --work NAME --preset P --children N --say "@all …" --dump-chrome FILE`.

Makima should feel like a raw terminal, as if you started tmux or zellij and it happened to know about your agents. No sidebar cards, no pills, no rounded components, no SF Pro. Everything on screen is monospaced text on the terminal's cell grid, coloured only from the terminal theme's ANSI palette.

## What we borrow

| From | Idea |
|---|---|
| **tmux** | A leader (prefix) key; a status line; box-drawing pane borders with the active pane highlighted; `synchronize-panes` to type into every pane at once, with a loud indicator while it's on |
| **zellij** | A mode indicator in the status bar; pane titles drawn into the top border; a compact layout |
| **Claude Code agent teams** | One lead session plus teammates; talking to any teammate directly; split panes need tmux or iTerm2, which is why Makima should look like tmux to Claude (see below) |
| **cmux** | A Ghostty-based Mac app that shows agents as native splits and puts a ring on panes that need attention; `cmux claude-teams` makes teammates appear as splits |
| **Claude Squad** | One agent per git worktree; a keyboard-first list of sessions |

## The screen

```text
 1:auth-refactor*  2:fix-ci  3:docs                                          makima
┌─ ◆ lead · claude ───────────────── working ─┐┌─ 1 · claude ───────────── working ─┐
│ ✻ Claude Code                               ││ Reading src/auth/session.rs…        │
│ > split the refactor into 3 tasks and       ││                                     │
│   spawn teammates                           │└─────────────────────────────────────┘
│                                             │┌─ 2 · claude ──────────── waiting ? ┐
│ ● Spawned 3 teammates                       ││ Allow edit to src/auth/token.rs?    │
│                                             ││ ❯ 1. Yes  2. No                     │
│                                             │└─────────────────────────────────────┘
│                                             │┌─ 3 · codex ─────────────── done ✓ ─┐
│                                             ││ All 214 tests passed.               │
└─────────────────────────────────────────────┘└─────────────────────────────────────┘
 @all ▸ stop and summarise what you changed_
 NOR │ auth-refactor │ ● 2 working  ? 1 waiting  ✓ 1 done │ Ctrl+Space ? keys │ 14:02
```

- **Tab line (top):** one tab per piece of work, numbered like tmux windows (`1:auth-refactor*`). It replaces the sidebar.
- **Panes:** real libghostty terminals with box-drawing borders. The title sits inside the top border: role marker (`◆ lead`, or the child's number), agent name, then status on the right. The focused pane's border is drawn in the accent colour; unfocused panes are dimmed (Ghostty's `unfocused-split-opacity`).
- **Layout:** the lead takes the left column and children stack on the right (tmux's `main-vertical`). It switches to a grid with more than four children, and zoom fills the screen with one pane (tmux's `z`).
- **Talk line:** a command line above the status bar. Its target is shown as a prefix: `@lead`, `@all`, `@children`, `@2`, `@claude-2`. Enter sends the text; Escape returns to the focused pane.
- **Status line:** mode (`INS`, `NOR`, `PREFIX`, `TALK`, `TOOL`), the current work, counts by status, a help hint and a clock, like tmux's `status-right`.
- **Attention:** a pane that needs input gets a `?` and an attention-coloured border, plus a mark on its tab, like cmux's notification ring.
- **Empty state:** no form. It's a shell-like prompt drawn in the same grid:
  ```text
  makima> new auth-refactor --lead claude --children 3
  ```
  with a few example commands dimmed underneath.

Everything is in the terminal font, at the terminal's cell size, using only the theme's ANSI colours: green = working, yellow = waiting, red = failed, blue = done, accent = focus.

## Leader and children

Each piece of work has **one lead and N children**.

- The **lead** is the agent you talk to by default. For Claude Code it runs in agent-team mode, so Claude itself can plan, spawn and coordinate.
- **Children** are the workers. Either Makima spawns them (`new … --children 3`, `space c`) or the lead spawns them as Claude Code teammates (below).

### Talking to them

| Target | Sends to |
|---|---|
| (focused pane) | Typing in a pane is just typing in that terminal, as in any terminal |
| `@lead` | The lead |
| `@all` | The lead and every child |
| `@children` | Every child |
| `@2`, `@claude-2` | One child, by number or name |

The talk line delivers text by **pasting it into each target's terminal and pressing Enter**. It uses bracketed paste through libghostty's `paste(text:)`, so multi-line prompts arrive whole. That works with any CLI agent (Claude Code, Codex, aider, a shell), the way tmux's `send-keys` does.

### Claude Code agent teams inside Makima

Claude Code puts teammates in split panes only through tmux or iTerm2, not Ghostty. So Makima should **look like tmux to Claude**:

1. Every Makima terminal gets `TMUX` / `TMUX_PANE` style variables and a `tmux` shim at the front of its `PATH`.
2. When the lead runs teammates with `--teammate-mode tmux`, its `tmux split-window`, `send-keys`, `list-panes` and similar calls reach the shim. The shim forwards them to Makima over a local socket.
3. Makima creates real child panes for those teammates, under the lead, with their names.

That's the same idea as `cmux claude-teams`. The exact tmux commands Claude uses have to be discovered by logging them through the shim first; the shim only needs to implement those.

## Keys

Two styles, picked with `[keys] style` in the config. Both start from the **leader key**, which is **Ctrl+Space** by default: no agent, shell or lazygit uses it, so the agents keep every key, including Esc, which Claude Code uses to interrupt. `[keys] leader` can also be `shift-esc`, `ctrl-]` or `ctrl-o` (which takes Claude's transcript toggle). ⌘Esc (`cmd-esc`) is accepted but doesn't work: macOS doesn't deliver it to apps as a key press. Hints and menus show the key you chose. ⌘ shortcuts in the Work menu stay as aliases; their titles show the active style's keys.

### Moving while typing

Ctrl+h/j/k/l moves between the lead and its children while you're typing in a pane, with no leader key, like vim-tmux-navigator. To talk to one agent, focus it and type: there's no talk line for that. Voice notes are typed into the focused agent's prompt, unsent, so you can read them first.

Ctrl+h/j/k/l are taken from the agents (in shells Ctrl+h is backspace and Ctrl+l clears), so `[keys] navigate = "alt"` uses Alt+h/j/k/l like Zellij, and `"off"` leaves moving to Makima's keys.

### Helix (default)

Modal, like Helix, with a split:
- **INS:** keys go to the focused agent, and Ctrl+h/j/k/l moves between agents.
- **NOR** (Ctrl+Space): everything that isn't an agent, meaning tools, terminals, files and works. It doesn't move between agents; that's Ctrl+h/j/k/l's job.
- **Popups:** NOR stays until you leave it. The status line shows the mode, and `space` and `o` show their keys in a popup at the bottom right.

| Keys (in NOR) | Action |
|---|---|
| `space g` `space s` `space e` `space f` | lazygit, a shell, `$EDITOR .`, yazi (any tool with a `key`); the same again hides it, keeping its state |
| `space o` | every tool (tools without a key get a digit) |
| `[` / `]` | previous / next work |
| `space b` | pick a work from a fuzzy list |
| `space n` / `space c` | new work / new child |
| `space a` | message everyone (`@2 …` for one child) |
| `space v` | push-to-talk |
| `space x` | close the focused agent |
| `space z` | zoom the focused agent (or the open tool) |
| `:` | the `makima>` prompt |
| `?` | all keys |
| `i` / `a` / `Enter` / `Esc` | back to the agent |
| `Ctrl+Space` | back to the agent (or to what you were typing) |

With `[keys] navigate = "off"`, `h j k l` move between agents in NOR instead, since Ctrl+h/j/k/l doesn't.

After a talk line, the picker or a `y/n` question, you're back in NOR.

### tmux

A prefix: the leader key (Ctrl+Space), then one key, then straight back to the pane. The status line shows `PREFIX` while it's armed.

| Keys | Action |
|---|---|
| leader then `c` / `n` | new child / new work |
| leader then `1`…`9` / `w` | jump to work 1…9 / pick a work |
| leader then `z` | zoom the focused pane |
| leader then `a` / `:` | message everyone / the `makima>` prompt |
| leader then `x` | close the agent (asks `y/n`) |
| leader then `v` / `?` | push-to-talk / all keys |
| leader then leader | (with `ctrl-space`) sends a literal Ctrl+Space to the pane |

## Tools

Programs you use alongside the agents open in a floating window over the work, in its folder. This follows tmux's `display-popup`, Zellij's floating panes and LazyVim's `<leader>gg`.

| Helix (in NOR) | tmux | Opens |
|---|---|---|
| `space g` | leader `g` | lazygit |
| `space s` | leader `s` | a shell |
| `space e` | leader `e` | `$EDITOR .` |
| `space f` | leader `f` | yazi |
| `space o` | leader `T` | the tools menu (every tool; tools without a key get a digit) |

- **Toggle:** the leader key (Ctrl+Space) inside a tool hides it and leaves you in NOR, ready for another tool (`space s`, `space o`) or `i` for the agent. From NOR, the tool's key toggles it too.
- **The window** is centred at 90% of the body, with a rounded border showing the tool's name, its folder and how to hide it.
- **While it's showing:**
  - keys go to the tool, except the leader key;
  - the panes behind are unfocused;
  - the status line reads `TOOL <name>`;
  - `space z` (Helix) or leader `z` (tmux) zooms it to the whole body.
- **Per work:** each work has its own copy of each tool, started in the work's folder.
  - Switching work, or any pane or work action, puts the tool away.
  - Hidden tools keep running (`keep = true`), so a shell keeps its history.
  - When the program exits, the window closes. Exit code 127 is reported as "isn't installed".
- **Config:** add tools, or replace a built-in one by id:

  ```toml
  [[tools]]
  id = "top"
  label = "btop"
  command = "btop"
  key = "t"          # optional, one free character
  size = 0.9         # 0.3 to 1
  keep = true
  ```

  Keys Makima already uses after `space` or the prefix are rejected, and so are two tools with the same key.
- **Implementation:**
  - Rust draws the border and returns the float's cell rect, the way it returns pane rects.
  - Swift places a libghostty terminal there, one per work and tool.
  - `MakimaApp --work x --tool git` opens a tool from the command line.

## How to build it: the chrome is a TUI too

To get a truly raw feel, the chrome shouldn't imitate a terminal. It should **be one**:

```text
┌──────────────── NSWindow ─────────────────────────────────────────┐
│  Chrome surface: one full-window libghostty surface fed by an      │
│  in-memory session. Rust renders tabs, borders, titles, talk line  │
│  and status with ratatui into ANSI, and Ghostty draws it.          │
│                                                                    │
│     ┌───────────────┐  ┌───────────────┐                           │
│     │ pane surface  │  │ pane surface  │  ← real exec terminals,    │
│     │ (exec backend)│  │ (exec backend)│    placed on top at the    │
│     └───────────────┘  └───────────────┘    cell rects ratatui's    │
│                                             layout computed         │
└────────────────────────────────────────────────────────────────────┘
```

- **Rust owns the TUI.** A `makima-tui` crate renders the chrome with **ratatui** into an ANSI byte stream, sized in cells. Its layout (lead column, child stack, grid, zoom) also returns each pane's inner rect in cells. The rendering is pure: state in, bytes and rects out, so it's easy to test with snapshots.
- **Swift hosts it.**
  - A full-window `TerminalView` with libghostty's `.inMemory` backend displays the chrome bytes.
  - Pane `TerminalView`s (the `.exec` backend) sit on top at `rect × cell size`.
  - Every surface shares one font and size, so everything lines up on one grid.
- **Input:**
  - In `INS` mode, keys go to the focused pane.
  - The leader key (Ctrl+Space) switches to `NOR` (or `PREFIX`), and keys go to Makima's key map in Rust (`Keymap`), named like Helix's (`h`, `space`, `C-w`, `esc`).
  - In `TALK` mode, the chrome surface's talk line takes the keys. Rust holds the line editor, the target parser (`@all …`) and history.
- **No SwiftUI in the main window.** The look comes entirely from the theme's ANSI colours and the font, so it can't drift from the terminals.

This keeps the Rust core in charge of state and layout, Swift as a thin host, and Ghostty doing all the drawing.

### Simpler fallback

Draw the borders, title lines and status line in Swift with Core Text at the terminal font's cell size, from the same ANSI palette. It's less pure, and harder to keep pixel-aligned with the terminals, but needs no in-memory surface. Use it only if the in-memory chrome surface has problems (input focus, z-order, performance).

## Phases

1. **Look.**
   - Remove the sidebar and SwiftUI chrome.
   - Add the ratatui chrome surface: tabs, box borders with titles, status line.
   - Lead/children layout and zoom.
   - Prefix key with the core actions.
2. **Talk.**
   - Talk line with `@lead`, `@all`, `@children`, `@N` targets, delivered by bracketed paste plus Enter.
   - Attention markers.
3. **Teams.**
   - The `tmux` shim and Makima socket.
   - Log which tmux commands `claude --teammate-mode tmux` uses, then implement them so teammates become child panes.
4. **Polish.**
   - Help overlay, the `makima>` empty-state prompt, resize behaviour, a pane-close confirm in the status line.
   - Run each child in its own git worktree, as Claude Squad does.

## Sources

- [Orchestrate teams of Claude Code sessions (Claude Code docs)](https://code.claude.com/docs/en/agent-teams)
- [Claude Code Swarms (Addy Osmani)](https://addyosmani.com/blog/claude-code-agent-teams/)
- [cmux: Ghostty-based macOS terminal for AI agents](https://github.com/manaflow-ai/cmux)
- [cmux issue #123: Claude Code agent teams in split panes](https://github.com/manaflow-ai/cmux/issues/123)
- [Claude Squad review](https://vibecodinghub.org/blog/claude-squad-review)
- [Claude Squad: worktrees meet agent orchestration](https://starlog.is/articles/ai-dev-tools/smtg-ai-claude-squad)
- [tmux synchronize-panes](https://ittavern.com/tmux-synchronize-the-input-of-all-panes-within-a-window/)
- [minimal-tmux-status (prefix indicator)](https://github.com/niksingh710/minimal-tmux-status/)
- [Make tmux pretty and usable](https://hamvocke.com/blog/a-guide-to-customizing-your-tmux-conf/)
- [Zellij layout examples](https://zellij.dev/documentation/layout-examples.html)
- [zjstatus: configurable zellij status bar](https://github.com/dj95/zjstatus)
