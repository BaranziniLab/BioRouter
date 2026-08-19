pub mod ingest;
pub mod lint;
pub mod query;

use crate::knowledge::service::KnowledgeService;

/// The two repairs every macro runs against a base before it works on it.
///
/// One function rather than the same eight lines at the top of `ingest`,
/// `query` and `lint`, because the two had already drifted once: the schema
/// migration's error was swallowed with `let _ =` in all three, which made a
/// failed migration indistinguishable from a base that needed none — and a
/// sub-agent then runs a whole ingest against a stale `schema.md`, producing
/// old-format pages, with nothing in the report to say why.
///
/// Neither repair is fatal, and that is deliberate:
///
/// - An **unmigrated schema** is worse than a current one and far better than a
///   refused ingest. The run continues against what is on disk.
/// - A **stale graph cache** costs a re-derive on the next read (which
///   `get_graph` now performs and rewrites), not correctness.
///
/// What is *not* acceptable is either of them failing quietly, which is what
/// `let _ =` bought.
pub(crate) fn refresh_base(svc: &KnowledgeService, kb_id: &str) {
    if let Err(e) = svc.migrate_schema_if_needed(kb_id) {
        tracing::warn!(
            "knowledge: could not migrate schema.md for '{kb_id}', continuing on the \
             schema already on disk: {e:#}"
        );
    }
    // Refresh the graph cache so any stale 0-edge cache produced by the
    // pre-fix wiki-link deriver is replaced with a freshly derived one.
    if let Err(e) = svc.rebuild_graph_cache(kb_id) {
        tracing::warn!("knowledge: could not rebuild the graph cache for '{kb_id}': {e:#}");
    }
}
