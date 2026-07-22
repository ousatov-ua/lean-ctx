//! Context Kernel integration for ctx_read hot-path.

/// Enrich a read result with cross-store context from the Context Kernel.
///
/// Returns `true` if enrichment was appended to `result`.
pub(super) fn enrich_with_kernel(result: &mut String, task: Option<&str>) -> bool {
    let (Some(task_str), Some(project_root)) =
        (task, crate::core::config::Config::find_project_root())
    else {
        return false;
    };

    let kernel_budget = 200;
    if let Some(enrichment) =
        crate::core::context_kernel::bridge::kernel_enrich(task_str, &project_root, kernel_budget)
        && !enrichment.blocks.is_empty()
    {
        result.push_str("\n--- kernel context ---\n");
        result.push_str(&enrichment.blocks);
        return true;
    }
    false
}
