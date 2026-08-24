use thiserror::Error;

/// Hidden stub for a verb removed in 1.0. `main` maps this to exit 2.
#[derive(Debug)]
pub struct RemovedVerb {
    pub message: String,
}

impl std::fmt::Display for RemovedVerb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RemovedVerb {}

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum Error {
    #[error("Claude Code config directory not found at {0}")]
    ClaudeDirMissing(std::path::PathBuf),

    #[error("Skill {0} is not installed")]
    SkillNotInstalled(String),

    #[error("Marketplace {0} is not registered")]
    MarketplaceNotFound(String),

    #[error("Skill spec {0} is ambiguous — qualify with @marketplace (matches: {1})")]
    AmbiguousSkill(String, String),
}
