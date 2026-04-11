use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};

use super::{exec::run_git_in, repo::repo_root};

pub fn hooks_path() -> Result<PathBuf> {
    let root = repo_root()?;
    let configured = run_git_in(&root, ["config", "core.hooksPath"]);
    let path = match configured {
        Ok(output) if !output.stdout.trim().is_empty() => PathBuf::from(output.stdout.trim()),
        _ => root.join(".git").join("hooks"),
    };

    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(root.join(path))
    }
}

pub fn write_hook(binary_path: &Path) -> Result<PathBuf> {
    let hook_path = hooks_path()?.join("prepare-commit-msg");
    if let Some(parent) = hook_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let script = format!(
        "#!/bin/sh\nexec \"{}\" hookrun \"$@\"\n",
        binary_path.display()
    );
    fs::write(&hook_path, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&hook_path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&hook_path, permissions)?;
    }

    Ok(hook_path)
}

pub fn remove_hook_if_owned(binary_path: &Path) -> Result<Option<PathBuf>> {
    let hook_path = hooks_path()?.join("prepare-commit-msg");
    if !hook_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&hook_path)?;
    if !content.contains(&binary_path.display().to_string()) || !content.contains("hookrun") {
        bail!("prepare-commit-msg already exists and is not managed by aic");
    }

    fs::remove_file(&hook_path)?;
    Ok(Some(hook_path))
}
