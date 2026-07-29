//! BR-71 workspace control: the single turn runner (Task 6) that both `/reply`
//! and detached/injected turns consume, the WorkspaceBridge and the services
//! impl (Slice 2). See docs/agent-loop/designs/agent-workspace-control.md.
pub mod services;
pub mod turn;
