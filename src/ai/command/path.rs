use std::{
    env,
    path::{Path, PathBuf},
};

pub(super) fn resolve_program_path(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 || path.is_absolute() {
        return path.exists().then(|| path.to_path_buf());
    }

    let path_var = env::var_os("PATH")?;
    for base in env::split_paths(&path_var) {
        for candidate in executable_candidates(&base, program) {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn executable_candidates(base: &Path, program: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let raw = base.join(program);
    candidates.push(raw.clone());

    #[cfg(windows)]
    {
        if Path::new(program).extension().is_none() {
            let pathext = env::var_os("PATHEXT")
                .unwrap_or_else(|| std::ffi::OsString::from(".COM;.EXE;.BAT;.CMD"));
            for ext in pathext
                .to_string_lossy()
                .split(';')
                .filter(|ext| !ext.is_empty())
            {
                let trimmed = ext.trim();
                let suffix = if trimmed.starts_with('.') {
                    trimmed.to_owned()
                } else {
                    format!(".{trimmed}")
                };
                candidates.push(base.join(format!("{program}{suffix}")));
            }
        }
    }

    candidates
}
