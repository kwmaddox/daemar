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
}

impl Role {
    pub const ALL: [Role; 3] = [Role::Scout, Role::Planner, Role::Responder];
}

/// What an agent may reach. Closed set; the write era adds variants here,
/// and the exhaustive matches will name every place that must decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAccess {
    None,
    ReadOnly,
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
            tools: ToolAccess::ReadOnly,
        },
        Role::Responder => AgentDef {
            name: "responder",
            system: "You are daemar's responder. Answer the request directly and \
                     completely, in plain text. If a plan is provided, follow it.",
            model_env: "DAEMAR_RESPOND_MODEL",
            tools: ToolAccess::None,
        },
    }
}
