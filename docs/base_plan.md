# Agent Orchestration Workspace

## Vision

Build a **local workspace for running multiple AI coding agents as a coordinated team**.

Instead of manually opening multiple terminals for Claude, Codex, shells, and different tasks, the application provides one environment where agents can:

- Be spawned and terminated
- Be grouped around specific work
- Communicate with each other
- Coordinate on larger tasks
- Be monitored in real time
- Be manually controlled whenever necessary

The goal is **not to build another terminal emulator or AI chat interface**.

The goal is to build an **operating environment for autonomous coding agents**.

---

## Core Experience

The application is a standalone desktop application built around **libghostty**.

The UI consists primarily of:

- Sidebar
- Workspaces
- Agent groups
- Agent terminals
- Workspace tabs
- Status/activity information

The terminal remains a first-class interface.

Every agent is ultimately something the user can open, observe, and interact with directly.

---

## Workspaces

A workspace represents a piece of work.

Examples:

- Authentication Refactor
- Database Migration
- Fix CI
- WebSocket Architecture Research
- Implement Billing

Each workspace maintains its own:

- Agents
- Agent groups
- Tasks
- Terminals
- Messages
- State
- Context
- Activity

Multiple workspaces can exist simultaneously.

Switching workspaces switches the entire agent environment rather than merely changing terminal tabs.

---

## Agent Groups

Agents can be organized into groups around particular responsibilities.

For example:

### Authentication

- Planner
- Backend Implementer
- Reviewer

### Frontend

- UI Implementer
- Test Agent

### Research

- Documentation Researcher
- Architecture Researcher

The conceptual hierarchy becomes:

Project
→ Workspace
→ Agent Group
→ Agent

The sidebar therefore represents the active organization of work rather than simply displaying terminal sessions.

---

## Agents

An agent is a **long-running worker managed by the orchestration system**.

Agents could be backed by:

- Claude Code
- Codex
- Other CLI coding agents
- Shell processes
- Custom agents

The orchestration system should avoid depending heavily on any particular agent implementation.

Every agent conceptually has:

- Identity
- Role
- Task
- Status
- Capabilities
- Workspace
- Group
- Lifecycle
- Communication channel

This allows different agent implementations to participate in the same workspace.

---

## Agent Communication

Agents should not behave like isolated terminal sessions.

They should be capable of communicating through the orchestration layer.

For example:

Implementer:

> Authentication middleware implementation is complete. Ready for review.

Reviewer:

> Session refresh contains a race condition. Sending the issue back.

Implementer:

> Fixing the session refresh implementation.

The orchestration system is responsible for routing these messages.

This turns the application from a **multi-terminal manager** into an actual **multi-agent system**.

---

## Coordination

Eventually, the user should be able to provide relatively high-level work.

For example:

> Refactor authentication to support workspace invitations.

The system could organize agents into roles.

### Planner

- Understands the repository
- Investigates the problem
- Creates an execution plan
- Splits work into tasks

### Implementer

- Receives implementation tasks
- Modifies the repository
- Reports progress

### Reviewer

- Reviews changes
- Detects architectural or implementation problems
- Sends issues back to implementers

### Test Agent

- Runs tests
- Investigates failures
- Reports regressions

Agents continue communicating until the task reaches a completion condition or requires human input.

---

## Human Control

The system should sit between:

**fully autonomous agents**

and

**manual terminal workflows**

The user should always understand what the system is doing.

Agent states might include:

- Starting
- Planning
- Working
- Waiting
- Waiting for Agent
- Reviewing
- Needs Human Input
- Failed
- Completed

Selecting an agent opens its actual terminal.

The system should never reduce everything to an opaque:

> AI is working...

The underlying agent process remains visible and interactive.

---

## Elixir

Elixir acts as the **control plane**.

It owns:

- Agent lifecycle
- Workspaces
- Agent groups
- Tasks
- Agent state
- Message routing
- Coordination
- Supervision
- Failure recovery
- Long-running workflows

A workspace can conceptually behave like its own supervision tree.

Agents are processes underneath that workspace.

If an individual agent crashes, the rest of the workspace should continue operating.

The orchestrator knows:

- What the agent was
- What it was doing
- Which task it owned
- Which agents depended on it
- Whether it should restart

BEAM therefore becomes the runtime responsible for keeping the agent system alive.

---

## Rust

Rust owns the **desktop application and interaction layer**.

It handles:

- Native application
- libghostty integration
- Terminal rendering
- Sidebar
- Tabs
- Panes
- Keyboard interaction
- Window management
- Workspace presentation
- Communication with the Elixir orchestrator

Rust should primarily present and control the system rather than contain orchestration logic.

Conceptually:

**Rust is the body.**

**Elixir is the nervous system.**

---

## Architecture Philosophy

Maintain a strong boundary between the desktop application and orchestration engine.

The orchestration system should eventually be capable of operating independently from the GUI.

This creates the possibility of:

- Headless orchestration
- Remote workspaces
- CLI control
- Multiple clients
- Persistent background agents
- Distributed agents

The desktop application becomes one interface into the orchestration system rather than the orchestration system itself.

---

## Long-Term Direction

The initial product may look like:

> A terminal application with multiple coordinated AI agents.

But the larger idea is:

> **A local operating system for software-engineering agents.**

You open a project.

You define pieces of work.

Agents organize around those pieces of work.

Some agents:

- Plan
- Research
- Implement
- Review
- Test
- Debug
- Coordinate
- Wait for other agents

The user moves between **workspaces of coordinated activity** instead of manually juggling terminal sessions.

The terminal remains available underneath everything, providing transparency and direct control.

The end result should feel less like running several AI CLIs and more like **operating a small engineering team from your desktop**.
