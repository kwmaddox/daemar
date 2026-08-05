//! The roster: who fills each seat.
//!
//! The decoupling (SSSF's best line, inherited): config defines who an agent
//! IS; the call site defines how it is USED. A workflow stage names a Role —
//! the seat it requires — and the roster binds that role to an Agent: persona,
//! model, tool access. Today the binding is 1:1 and lives in Rust (the
//! compiler is the first gate). When multiple candidates per role exist, this
//! table becomes data (`roster.toml` beside `airframes.toml`) and bindings
//! can be assigned per flight — by policy, or someday by measured competence.

/// The seats workflows can require. Closed set: adding a role is a variant,
/// and the compiler walks you to every match that must care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Scout,
    Planner,
    Responder,
    Builder,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Scout, Role::Planner, Role::Responder, Role::Builder];
}

/// What an agent may reach. Closed set; the write era adds variants here,
/// and the exhaustive matches will name every place that must decide.
/// Serde: the access rides inside cage tool requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAccess {
    None,
    ReadOnly,
    /// Read plus the write primitives (edit, write). A seat with this
    /// access flies caged UNCONDITIONALLY — write tools were born inside
    /// the cage and have never existed outside it.
    ReadWrite,
}

/// Who an agent IS: identity, persona, model binding, tool access.
/// Everything a stage does with the agent (phase name, section kind, task
/// prompt) belongs to the stage, not here.
pub struct AgentDef {
    /// The name recorded as `owner` on the ledger.
    pub name: &'static str,
    pub system: &'static str,
    /// Env var that binds this agent's airframe; DAEMAR_MODEL is the fallback.
    pub model_env: &'static str,
    /// Env var that binds this agent's reasoning effort; DAEMAR_EFFORT is
    /// the fallback, and `medium` the default when neither is set.
    pub effort_env: &'static str,
    pub tools: ToolAccess,
}

pub fn agent(role: Role) -> AgentDef {
    match role {
        Role::Scout => AgentDef {
            name: "scout",
            system: "You are daemar's scout: read-only reconnaissance over one repository \
                     (the territory). Use the tools to find where things live and how they \
                     connect. Never guess file contents — read them. When you have enough \
                     evidence, reply in plain text with your findings: what lives where \
                     (cite paths), how the pieces connect, and anything surprising. Be \
                     concise and concrete.",
            model_env: "DAEMAR_SCOUT_MODEL",
            effort_env: "DAEMAR_SCOUT_EFFORT",
            tools: ToolAccess::ReadOnly,
        },
        Role::Planner => AgentDef {
            name: "planner",
            system: "You are daemar's planner. Investigate the territory with the tools \
                     before planning — never plan from imagination: read the files your \
                     plan will touch. Then produce a concise, implementable plan that \
                     cites real paths (with line references where they help): what to \
                     change, where, in what order, and how to verify. Do not implement \
                     anything.",
            model_env: "DAEMAR_PLAN_MODEL",
            effort_env: "DAEMAR_PLAN_EFFORT",
            tools: ToolAccess::ReadOnly,
        },
        Role::Responder => AgentDef {
            name: "responder",
            system: "You are daemar's responder. Answer the request directly and \
                     completely, in plain text. If a plan is provided, follow it.",
            model_env: "DAEMAR_RESPOND_MODEL",
            effort_env: "DAEMAR_RESPOND_EFFORT",
            tools: ToolAccess::None,
        },
        Role::Builder => AgentDef {
            name: "builder",
            system: "You are daemar's builder: careful modification of one \
                     repository worktree. Read before you touch — edit refuses \
                     files you have not read at their current content. Make the \
                     smallest change that satisfies the request, match the \
                     surrounding code's style exactly, and verify your work by \
                     re-reading what you changed. When you are done, reply in \
                     plain text describing exactly what you changed and why — \
                     the diff itself is computed and reviewed separately.",
            model_env: "DAEMAR_BUILD_MODEL",
            effort_env: "DAEMAR_BUILD_EFFORT",
            tools: ToolAccess::ReadWrite,
        },
    }
}
