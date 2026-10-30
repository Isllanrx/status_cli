use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::same_dir;

pub const SHIM_VAR: &str = "STATUS_CLI_SHIM";
pub const GUARD_VAR: &str = "STATUS_CLI_IN_CODEX";

const PLAIN_FLAGS: [&str; 4] = ["-h", "--help", "-V", "--version"];
const SUBCOMMANDS: [&str; 32] = [
    "agents",
    "tcp-tunnel",
    "exec",
    "e",
    "review",
    "login",
    "logout",
    "mcp",
    "plugin",
    "app-server",
    "remote-control",
    "app",
    "completion",
    "update",
    "doctor",
    "sandbox",
    "debug",
    "execpolicy",
    "apply",
    "a",
    "queue",
    "archive",
    "delete",
    "migrate-rollouts",
    "migrate-nfs-db",
    "unarchive",
    "cloud",
    "cloud-tasks",
    "responses-api-proxy",
    "stdio-to-uds",
    "exec-server",
    "features",
];

pub fn interactive(args: &[String]) -> bool {
    if args.iter().any(|arg| PLAIN_FLAGS.contains(&arg.as_str())) {
        return false;
    }
    args.iter().find(|arg| !arg.starts_with('-')).is_none_or(|first| !SUBCOMMANDS.contains(&first.as_str()))
}

pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, OsString)>,
}

pub fn command(args: &[String], shim: Option<&Path>) -> Launch {
    let mut env = vec![(GUARD_VAR.to_owned(), OsString::from("1"))];
    if let (Some(shim), Some(path)) = (shim, env::var_os("PATH"))
        && let Some(path) = without(&path, shim)
    {
        env.push(("PATH".to_owned(), path));
    }
    let (program, args) = match env::var("STATUS_CLI_CODEX") {
        Ok(program) => (program, args.to_vec()),
        Err(_) if cfg!(windows) => {
            let mut wrapped = vec!["/d".to_owned(), "/c".to_owned(), "codex".to_owned()];
            wrapped.extend_from_slice(args);
            ("cmd".to_owned(), wrapped)
        }
        Err(_) => ("codex".to_owned(), args.to_vec()),
    };
    Launch { program, args, env }
}

pub fn installed_status_cli(shim: &Path) -> Option<PathBuf> {
    let name = format!("status_cli{}", env::consts::EXE_SUFFIX);
    env::split_paths(&env::var_os("PATH")?)
        .filter(|dir| !same_dir(dir, shim))
        .map(|dir| dir.join(&name))
        .find(|exe| exe.is_file())
}

fn without(path: &OsString, dir: &Path) -> Option<OsString> {
    env::join_paths(env::split_paths(path).filter(|entry| !same_dir(entry, dir))).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_child_path_drops_only_the_shim_folder() {
        let shim = env::temp_dir().join("status_cli-shim");
        let other = env::temp_dir().join("npm");
        let path = env::join_paths([shim.clone(), other.clone()]).unwrap();
        assert_eq!(without(&path, &shim), Some(env::join_paths([other]).unwrap()));
    }

    #[test]
    fn only_the_interactive_tui_is_wrapped() {
        let args = |list: &[&str]| list.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        assert!(interactive(&args(&[])));
        assert!(interactive(&args(&["--model", "gpt-6", "fix the build"])));
        assert!(interactive(&args(&["resume", "--last"])));
        assert!(interactive(&args(&["fork"])));
        assert!(!interactive(&args(&["exec", "ls"])));
        assert!(!interactive(&args(&["--full-auto", "e", "ls"])));
        assert!(!interactive(&args(&["login"])));
        assert!(!interactive(&args(&["--version"])));
        assert!(!interactive(&args(&["resume", "--help"])));
    }
}
