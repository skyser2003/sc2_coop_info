use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvAssignment {
    name: String,
    value: String,
}

impl EnvAssignment {
    pub fn new(name: String, value: String) -> Self {
        Self { name, value }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Error)]
pub enum EnvFileError {
    #[error("failed to read env file '{0}': {1}")]
    ReadFailed(PathBuf, #[source] std::io::Error),
}

pub struct EnvFileLoader;

impl EnvFileLoader {
    pub fn load_repo_env_files(repo_root: &Path) -> Result<Vec<EnvAssignment>, EnvFileError> {
        let mut assignments = Vec::new();
        assignments.extend(Self::load_if_exists(&repo_root.join(".env"))?);
        assignments.extend(Self::load_if_exists(&repo_root.join(".envrc"))?);
        for assignment in &assignments {
            Self::apply_assignment(assignment);
        }
        Ok(assignments)
    }

    pub fn parse_content(content: &str) -> Vec<EnvAssignment> {
        content
            .lines()
            .filter_map(Self::parse_line)
            .collect::<Vec<EnvAssignment>>()
    }

    fn load_if_exists(path: &Path) -> Result<Vec<EnvAssignment>, EnvFileError> {
        if !path.is_file() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(path)
            .map_err(|error| EnvFileError::ReadFailed(path.into(), error))?;
        Ok(Self::parse_content(&content))
    }

    fn parse_line(line: &str) -> Option<EnvAssignment> {
        let mut trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        if let Some(rest) = trimmed.strip_prefix("export ") {
            trimmed = rest.trim();
        }

        let (name, raw_value) = trimmed.split_once('=')?;
        let name = name.trim();
        if name.is_empty() {
            return None;
        }

        Some(EnvAssignment::new(
            name.to_string(),
            Self::unquote_value(raw_value.trim()),
        ))
    }

    fn unquote_value(value: &str) -> String {
        let double_quoted = value.starts_with('"') && value.ends_with('"') && value.len() >= 2;
        let single_quoted = value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2;
        if double_quoted || single_quoted {
            value[1..value.len() - 1].to_string()
        } else {
            value.to_string()
        }
    }

    fn apply_assignment(assignment: &EnvAssignment) {
        // The CLI loads env files during startup, before it creates worker threads.
        unsafe {
            std::env::set_var(assignment.name(), assignment.value());
        }
    }
}
