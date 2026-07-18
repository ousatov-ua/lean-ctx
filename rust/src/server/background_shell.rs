//! Explicit, in-process background jobs for `ctx_shell`.
//!
//! The MCP request may complete immediately, but the child keeps the exact
//! timeout, allow-list, path-jail and process-group policy of foreground shell
//! execution. Jobs intentionally live in the daemon: restarting it invalidates
//! outstanding jobs rather than silently orphaning subprocesses.

use std::collections::HashMap;
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobState {
    Running,
    Completed { output: String, exit_code: i32 },
    Cancelled { output: String },
}

struct Job {
    cancel: Arc<AtomicBool>,
    state: JobState,
}

static JOBS: LazyLock<Mutex<HashMap<String, Job>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn start(
    command: String,
    cwd: String,
    extra_env: std::collections::HashMap<String, String>,
    timeout_ms: Option<u64>,
) -> String {
    // IDs are content-addressed so tool responses stay deterministic (#498).
    // An identical in-flight launch coalesces onto the same job instead of
    // creating duplicate expensive builds/tests.
    let mut env_entries: Vec<_> = extra_env.iter().collect();
    env_entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let env_key = env_entries
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\0");
    let material = format!(
        "{command}\0{cwd}\0{}\0{env_key}",
        timeout_ms.unwrap_or_default()
    );
    let id = format!(
        "shell_{}",
        &blake3::hash(material.as_bytes()).to_hex()[..16]
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    {
        let mut jobs = JOBS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(jobs.get(&id).map(|job| &job.state), Some(JobState::Running)) {
            return id;
        }
        jobs.insert(
            id.clone(),
            Job {
                cancel,
                state: JobState::Running,
            },
        );
    }

    let worker_id = id.clone();
    std::thread::spawn(move || {
        let (output, exit_code) = crate::server::execute::execute_command_with_env_cancellable(
            &command,
            &cwd,
            &extra_env,
            timeout_ms,
            Some(&worker_cancel),
        );
        let mut jobs = JOBS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(job) = jobs.get_mut(&worker_id) else {
            return;
        };
        job.state = if worker_cancel.load(Ordering::Acquire) {
            JobState::Cancelled { output }
        } else {
            JobState::Completed { output, exit_code }
        };
    });
    id
}

pub fn status(id: &str) -> Option<JobState> {
    JOBS.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(id)
        .map(|job| job.state.clone())
}

pub fn cancel(id: &str) -> Option<JobState> {
    let mut jobs = JOBS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let job = jobs.get_mut(id)?;
    if matches!(job.state, JobState::Running) {
        job.cancel.store(true, Ordering::Release);
    }
    Some(job.state.clone())
}

#[cfg(test)]
mod tests {
    use super::{JobState, cancel, start, status};
    use std::time::Duration;

    #[test]
    #[cfg_attr(windows, ignore)]
    fn background_job_runs_past_request_and_can_be_observed() {
        let id = start(
            "sleep 0.1; printf BG_JOB_OK".to_string(),
            ".".to_string(),
            std::collections::HashMap::default(),
            Some(10_000),
        );
        assert_eq!(status(&id), Some(JobState::Running));
        for _ in 0..40 {
            if let Some(JobState::Completed { output, exit_code }) = status(&id) {
                assert_eq!(exit_code, 0);
                assert!(output.contains("BG_JOB_OK"));
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("background job did not complete");
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    fn cancelling_background_job_returns_cancelled_state() {
        let id = start(
            "sleep 5".to_string(),
            ".".to_string(),
            std::collections::HashMap::default(),
            Some(10_000),
        );
        assert!(matches!(cancel(&id), Some(JobState::Running)));
        for _ in 0..40 {
            if let Some(JobState::Cancelled { output }) = status(&id) {
                assert!(output.contains("command cancelled"));
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("background job was not cancelled");
    }
}
