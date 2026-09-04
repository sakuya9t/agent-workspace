//! Tell agents about ASM's branch ownership without editing repository files.
//! This is guidance, not a Git permission boundary.

use std::path::Path;

use anyhow::{anyhow, Context, Result};

use crate::plugins::LaunchSpec;
use crate::source_control::current_branch;

pub(super) fn apply(
    plugin: &str,
    launch: &mut LaunchSpec,
    cwd: &Path,
    tracked_branch: Option<&str>,
    instruction_file: &Path,
) -> Result<()> {
    if !matches!(plugin, "codex" | "claude" | "pi" | "opencode") {
        return Ok(());
    }
    let branch = tracked_branch
        .map(str::to_owned)
        .or_else(|| current_branch(cwd));
    let Some(branch) = branch else {
        // Plain directories and detached checkouts have no branch to preserve.
        return Ok(());
    };
    let instruction = format!(
        "This session is managed by Agent Session Manager (ASM).\n\
         Assigned workspace directory: {cwd}\n\
         Tracked workspace branch: {branch}\n\
         These are the current session's destination and supersede locations remembered from an earlier session. \
         Keep all work, edits, and commits in this directory on this branch so ASM can track them. \
         This applies during planning, entering or leaving plan mode, and implementing a plan. \
         Do not create or switch to a feature branch, detach HEAD, or create another worktree \
         as part of your workflow. Any plan you write must keep implementation on the assigned branch. \
         A general request to plan or implement does not authorize changing branches or worktrees; \
         only an explicit user request to move the work does. \
         Before editing or committing, verify the current branch matches the tracked branch. \
         If it differs, report the mismatch and wait for direction instead of moving or resetting work.",
        cwd = cwd.display(),
    );

    let args = match plugin {
        // A JSON string is also a TOML basic string for this text. Pass directly
        // as argv: paths / branch names are never interpolated into shell code.
        "codex" => vec![
            "-c".into(),
            format!(
                "developer_instructions={}",
                serde_json::to_string(&instruction)?
            ),
        ],
        "claude" | "pi" => {
            // Claude rejects repeated occurrences of this flag. Preserve a
            // caller's existing append prompt and add ASM's context to it.
            let flag = "--append-system-prompt";
            for index in 0..launch.args.len() {
                if launch.args[index] == "--" {
                    break;
                }
                if launch.args[index] == flag && index + 1 < launch.args.len() {
                    launch.args[index + 1].push_str("\n\n");
                    launch.args[index + 1].push_str(&instruction);
                    return Ok(());
                }
                if launch.args[index].starts_with(&format!("{flag}=")) {
                    launch.args[index].push_str("\n\n");
                    launch.args[index].push_str(&instruction);
                    return Ok(());
                }
            }
            vec![flag.into(), instruction]
        }
        "opencode" => {
            // OpenCode loads instruction files through its config. Keep the
            // file outside the worktree, and retain it for resumed sessions.
            // Preserve any other inline config supplied for this launch.
            const KEY: &str = "OPENCODE_CONFIG_CONTENT";
            let existing = launch
                .env
                .iter()
                .rev()
                .find(|(k, _)| k == KEY)
                .map(|(_, v)| v.clone())
                .or_else(|| std::env::var(KEY).ok());
            let mut config: serde_json::Value = match existing {
                Some(value) => {
                    serde_json::from_str(&value).context("invalid OPENCODE_CONFIG_CONTENT JSON")?
                }
                None => serde_json::json!({}),
            };
            let object = config
                .as_object_mut()
                .ok_or_else(|| anyhow!("OPENCODE_CONFIG_CONTENT must be an object"))?;
            let instructions = object
                .entry("instructions")
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut()
                .ok_or_else(|| anyhow!("OpenCode instructions must be an array"))?;
            instructions.push(serde_json::json!(instruction_file));
            std::fs::create_dir_all(instruction_file.parent().unwrap())?;
            std::fs::write(instruction_file, instruction)?;
            launch.env.retain(|(k, _)| k != KEY);
            launch
                .env
                .push((KEY.into(), serde_json::to_string(&config)?));
            return Ok(());
        }
        _ => unreachable!(),
    };
    // Global flags must precede subcommands, opening prompts and `--`.
    launch.args.splice(0..0, args);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(args: &[&str]) -> LaunchSpec {
        LaunchSpec {
            command: "agent".into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: vec![],
            requires_approval: false,
        }
    }

    #[test]
    fn fresh_and_fork_launches_receive_destination_branch_before_positionals() {
        for plugin in ["codex", "claude", "pi"] {
            for original in [vec!["--", "user prompt"], vec!["fork", "origin", "seed"]] {
                let mut spec = launch(&original);
                apply(
                    plugin,
                    &mut spec,
                    Path::new("/workspace with spaces"),
                    Some("session/destination"),
                    Path::new("/unused"),
                )
                .unwrap();
                assert_eq!(&spec.args[2..], original);
                let text = if plugin == "codex" {
                    serde_json::from_str::<String>(
                        spec.args[1]
                            .strip_prefix("developer_instructions=")
                            .unwrap(),
                    )
                    .unwrap()
                } else {
                    spec.args[1].clone()
                };
                assert!(text.contains("session/destination"));
                assert!(text.contains("/workspace with spaces"));
                assert!(text.contains("entering or leaving plan mode"));
                assert!(text.contains("only an explicit user request"));
            }
        }
    }

    #[test]
    fn shell_and_custom_commands_are_not_given_agent_flags() {
        for plugin in ["shell", "custom_command"] {
            let mut spec = launch(&["-i"]);
            apply(
                plugin,
                &mut spec,
                Path::new("/unused"),
                Some("main"),
                Path::new("/unused"),
            )
            .unwrap();
            assert_eq!(spec.args, ["-i"]);
            assert!(spec.env.is_empty());
        }
    }

    #[test]
    fn existing_append_prompt_is_preserved_without_duplicate_flags() {
        for plugin in ["claude", "pi"] {
            let mut spec = launch(&["--append-system-prompt", "User instructions", "prompt"]);
            apply(
                plugin,
                &mut spec,
                Path::new("/workspace"),
                Some("tracked"),
                Path::new("/unused"),
            )
            .unwrap();
            assert_eq!(spec.args.len(), 3);
            assert!(spec.args[1].starts_with("User instructions\n\n"));
            assert!(spec.args[1].contains("Tracked workspace branch: tracked"));
            assert_eq!(spec.args[2], "prompt");
        }
    }

    #[test]
    fn plain_directory_has_no_branch_instructions() {
        let dir = std::env::temp_dir().join(format!("asm-plain-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for plugin in ["codex", "claude", "pi", "opencode"] {
            let mut spec = launch(&["user prompt"]);
            apply(plugin, &mut spec, &dir, None, &dir.join("unused.md")).unwrap();
            assert_eq!(spec.args, ["user prompt"]);
            assert!(spec.env.is_empty());
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn opencode_preserves_config_and_keeps_instructions_outside_repo() {
        let dir = std::env::temp_dir().join(format!("asm-instructions-{}", uuid::Uuid::new_v4()));
        let file = dir.join("instructions.md");
        let mut spec = launch(&["--prompt", "read brief"]);
        spec.env.push((
            "OPENCODE_CONFIG_CONTENT".into(),
            r#"{"model":"provider/model","instructions":["existing.md"]}"#.into(),
        ));
        apply(
            "opencode",
            &mut spec,
            Path::new("/workspace"),
            Some("tracked"),
            &file,
        )
        .unwrap();
        let config: serde_json::Value = serde_json::from_str(&spec.env[0].1).unwrap();
        assert_eq!(config["model"], "provider/model");
        assert_eq!(
            config["instructions"],
            serde_json::json!(["existing.md", file])
        );
        assert!(std::fs::read_to_string(&file)
            .unwrap()
            .contains("Tracked workspace branch: tracked"));
        assert_eq!(spec.args, ["--prompt", "read brief"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
