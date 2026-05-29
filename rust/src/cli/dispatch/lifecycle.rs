// Auto-split from the former monolithic dispatch.rs. run() (the command
// match) stays in mod.rs; standalone helpers grouped by concern.

use crate::core;

pub(super) fn cmd_stop() {
    use crate::daemon;
    use crate::ipc;

    eprintln!("Stopping all lean-ctx processes…");

    crate::proxy_autostart::stop();
    crate::daemon_autostart::stop();
    eprintln!("  Unloaded autostart (LaunchAgent/systemd).");

    // 2. Stop daemon via IPC
    if let Err(e) = daemon::stop_daemon() {
        eprintln!("  Warning: daemon stop: {e}");
    }

    // 3. SIGTERM all remaining lean-ctx processes
    let killed = ipc::process::kill_all_by_name("lean-ctx");
    if killed > 0 {
        eprintln!("  Sent SIGTERM to {killed} process(es).");
    }

    std::thread::sleep(std::time::Duration::from_millis(500));

    // 4. Force-kill stragglers (but never MCP servers — IDE will respawn them)
    let remaining = ipc::process::find_killable_pids("lean-ctx");
    if !remaining.is_empty() {
        eprintln!("  Force-killing {} stubborn process(es)…", remaining.len());
        for &pid in &remaining {
            let _ = ipc::process::force_kill(pid);
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    daemon::cleanup_daemon_files();

    let final_check = ipc::process::find_killable_pids("lean-ctx");
    if final_check.is_empty() {
        eprintln!("  ✓ All lean-ctx processes stopped.");
    } else {
        eprintln!(
            "  ✗ {} process(es) could not be killed: {:?}",
            final_check.len(),
            final_check
        );
        eprintln!(
            "    Try: sudo kill -9 {}",
            final_check
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        );
        std::process::exit(1);
    }
}

pub(super) fn cmd_restart() {
    use crate::daemon;
    use crate::ipc;

    eprintln!("Restarting lean-ctx…");

    crate::proxy_autostart::stop();
    crate::daemon_autostart::stop();

    if let Err(e) = daemon::stop_daemon() {
        eprintln!("  Warning: daemon stop: {e}");
    }

    let orphans = ipc::process::kill_all_by_name("lean-ctx");
    if orphans > 0 {
        eprintln!("  Terminated {orphans} orphan process(es).");
    }

    std::thread::sleep(std::time::Duration::from_millis(500));

    let remaining = ipc::process::find_killable_pids("lean-ctx");
    if !remaining.is_empty() {
        eprintln!(
            "  Force-killing {} stubborn process(es): {:?}",
            remaining.len(),
            remaining
        );
        for &pid in &remaining {
            let _ = ipc::process::force_kill(pid);
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    daemon::cleanup_daemon_files();

    crate::proxy_autostart::start();

    if crate::daemon_autostart::is_installed() {
        crate::daemon_autostart::start();
        eprintln!("  ✓ Daemon restarted via autostart.");
    } else {
        match daemon::start_daemon(&[]) {
            Ok(()) => eprintln!("  ✓ Daemon restarted."),
            Err(e) => {
                eprintln!("  ✗ Daemon start failed: {e}");
                std::process::exit(1);
            }
        }
    }
}

pub(super) fn cmd_dev_install() {
    use crate::ipc;

    let cargo_root = find_cargo_project_root();
    let Some(cargo_root) = cargo_root else {
        eprintln!("Error: No Cargo.toml found. Run from the lean-ctx project directory.");
        std::process::exit(1);
    };

    eprintln!("Building release binary…");
    let build = std::process::Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&cargo_root)
        .status();

    match build {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("  Build failed with exit code {}", s.code().unwrap_or(-1));
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("  Build failed: {e}");
            std::process::exit(1);
        }
    }

    let built_binary = cargo_root.join("target/release/lean-ctx");
    if !built_binary.exists() {
        eprintln!(
            "  Error: Built binary not found at {}",
            built_binary.display()
        );
        std::process::exit(1);
    }

    let install_path = resolve_install_path();
    eprintln!("Installing to {}…", install_path.display());

    eprintln!("  Stopping all lean-ctx processes…");
    crate::proxy_autostart::stop();
    crate::daemon_autostart::stop();
    let _ = crate::daemon::stop_daemon();
    ipc::process::kill_all_by_name("lean-ctx");
    std::thread::sleep(std::time::Duration::from_millis(500));

    let remaining = ipc::process::find_pids_by_name("lean-ctx");
    if !remaining.is_empty() {
        eprintln!("  Force-killing {} stubborn process(es)…", remaining.len());
        for &pid in &remaining {
            let _ = ipc::process::force_kill(pid);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    let old_path = install_path.with_extension("old");
    if install_path.exists() {
        if let Err(e) = std::fs::rename(&install_path, &old_path) {
            eprintln!("  Warning: rename existing binary: {e}");
        }
    }

    match std::fs::copy(&built_binary, &install_path) {
        Ok(_) => {
            let _ = std::fs::remove_file(&old_path);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&install_path, std::fs::Permissions::from_mode(0o755));
            }
            eprintln!("  ✓ Binary installed.");
        }
        Err(e) => {
            eprintln!("  Error: copy failed: {e}");
            if old_path.exists() {
                let _ = std::fs::rename(&old_path, &install_path);
                eprintln!("  Rolled back to previous binary.");
            }
            std::process::exit(1);
        }
    }

    let version = std::process::Command::new(&install_path)
        .arg("--version")
        .output()
        .map_or_else(
            |_| "unknown".to_string(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
        );

    eprintln!("  ✓ dev-install complete: {version}");

    eprintln!("  Re-enabling autostart…");
    crate::proxy_autostart::start();

    if crate::daemon_autostart::is_installed() {
        crate::daemon_autostart::start();
        eprintln!("  ✓ Daemon restarted via autostart.");
    } else {
        eprintln!("  Starting daemon…");
        match crate::daemon::start_daemon(&[]) {
            Ok(()) => {}
            Err(e) => eprintln!("  Warning: daemon start: {e} (will be started by editor)"),
        }
    }
}

pub(super) fn find_cargo_project_root() -> Option<std::path::PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join("Cargo.toml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub(super) fn resolve_install_path() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(canonical) = exe.canonicalize() {
            let is_in_cargo_target = canonical.components().any(|c| c.as_os_str() == "target");
            if !is_in_cargo_target && canonical.exists() {
                return canonical;
            }
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let local_bin = std::path::PathBuf::from(&home).join(".local/bin/lean-ctx");
        if local_bin.parent().is_some_and(std::path::Path::exists) {
            return local_bin;
        }
    }

    std::path::PathBuf::from("/usr/local/bin/lean-ctx")
}

pub(super) fn spawn_proxy_if_needed() {
    use std::net::TcpStream;

    let cfg = core::config::Config::load();
    if cfg.proxy_enabled != Some(true) {
        return;
    }

    let port = crate::proxy_setup::default_port();
    let already_running = {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        TcpStream::connect_timeout(&addr, crate::proxy_setup::proxy_timeout()).is_ok()
    };

    if already_running {
        tracing::debug!("proxy already running on port {port}");
        return;
    }

    let binary = core::portable_binary::resolve_portable_binary();

    match std::process::Command::new(&binary)
        .args(["proxy", "start", &format!("--port={port}")])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => tracing::info!("auto-started proxy on port {port}"),
        Err(e) => tracing::debug!("could not auto-start proxy: {e}"),
    }
}
