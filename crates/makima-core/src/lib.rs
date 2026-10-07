//! Makima's domain model: workspaces, agents, configuration and design
//! tokens.
//!
//! This crate is synchronous and free of I/O on purpose: it holds the rules,
//! and the FFI, CLI and orchestrator layers drive it and do the I/O.
//!
//! API layout: the domain model (workspaces, agents, presets, the registry)
//! is re-exported at the crate root. Self-contained subsystems — [`brief`],
//! [`color`], [`config`] and [`tokens`] — are public modules used by path, e.g.
//! `makima_core::config::Config`.

mod adapter;
mod agent;
pub mod brief;
pub mod color;
pub mod config;
mod error;
mod ids;
mod kind;
mod permission;
mod preset;
pub mod prompt;
mod registry;
mod template;
pub mod tokens;
mod tool;
mod workspace;

pub use adapter::{
    AgentAdapter, ClaudeCode, Codex, Configured, SessionStore, Shell, UnknownAdapter,
    builtin_adapter,
};
pub use agent::{Agent, AgentRole, AgentStatus};
pub use error::RegistryError;
pub use ids::{AgentId, WorkspaceId};
pub use kind::{UnknownKind, WorkKind};
pub use permission::{PermissionFlags, PermissionMode, UnknownPermission};
pub use preset::{
    AgentPreset, DEFAULT_LEAD, Launch, PresetId, builtin_presets, merge_presets, shell_quote,
};
pub use registry::Registry;
pub use template::{BriefQuestion, TeamMember, Template, builtin_templates, merge_templates};
pub use tool::{RESERVED_TOOL_KEYS, Tool, builtin_tools, merge_tools};
pub use workspace::{Workspace, WorkspaceName};
