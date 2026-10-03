use std::path::PathBuf;

use asset_transport::{ensure_artifacts_dir, games_root_from_env};

#[global_allocator]
static PROCESS_ALLOCATOR: diag::ProcessCountingAllocator = diag::ProcessCountingAllocator;

const LICENSES: [(&str, &str); 5] = [
    ("LICENSE", include_str!("../../../LICENSE")),
    ("NOTICE", include_str!("../../../NOTICE")),
    (
        "OFL-Oxanium.txt",
        include_str!("../../ui/assets/OFL-Oxanium.txt"),
    ),
    (
        "COPYING-FreeFont.txt",
        include_str!("../../console/assets/COPYING-FreeFont.txt"),
    ),
    (
        "THIRD-PARTY-LICENSES.txt",
        include_str!("../../../THIRD-PARTY-LICENSES.txt"),
    ),
];

fn main() {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "licenses")
    {
        for (name, text) in LICENSES {
            println!("==> {name} <==\n\n{text}\n");
        }
        return;
    }
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == zone_inspect::COMMAND)
    {
        std::process::exit(inspect_zone());
    }
    let mut args = match updater::startup().unwrap_or_else(|e| diag::exit_launch_error(&e)) {
        Some(args) => args,
        None => return,
    };
    if args.is_empty() {
        args.push("menu".into());
    }
    bootstrap::bench::arm();
    prepare_process_root().unwrap_or_else(|e| {
        diag::exit_launch_error(&e);
    });
    let artifacts = ensure_artifacts_dir().unwrap_or_else(|e| diag::exit_launch_error(&e));
    announce_log(diag::init_log(&artifacts));
    let (mode, acceptance, cheats) = bootstrap::parse_cli(
        args.into_iter()
            .map(|arg| arg.to_string_lossy().into_owned()),
    )
    .unwrap_or_else(|e| diag::exit_launch_error(&e));
    let games = games_root_from_env().unwrap_or_else(|e| diag::exit_launch_error(&e));
    bootstrap::launch(games, artifacts, mode, acceptance, cheats);
}

fn inspect_zone() -> i32 {
    let args: Vec<String> = std::env::args_os()
        .skip(2)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let artifacts = match ensure_artifacts_dir() {
        Ok(artifacts) => {
            announce_log(diag::init_log(&artifacts));
            Some(artifacts)
        }
        Err(error) => {
            diag::announce_stdout(&format!(
                "inspect: warn reason=\"no artifacts folder ({}); reports and the log are not written\"",
                error.replace('"', "'")
            ));
            None
        }
    };
    let code = zone_inspect::run(&args, artifacts.as_deref(), &mut std::io::stdout().lock());
    diag::flush();
    code
}

fn prepare_process_root() -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe =
            std::env::current_exe().map_err(|error| format!("cannot locate iw4l.exe: {error}"))?;
        let root = exe
            .parent()
            .ok_or_else(|| format!("iw4l.exe has no parent directory: {}", exe.display()))?;
        std::env::set_current_dir(root).map_err(|error| {
            format!(
                "cannot enter launcher directory {}: {error}",
                root.display()
            )
        })?;
    }
    Ok(())
}

fn announce_log(path: PathBuf) {
    diag::announce_log_stdout(&path, diag::latest_log_path().as_deref());
    diag::info!(Launch, "log: {}", path.display());
}
