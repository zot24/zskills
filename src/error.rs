use thiserror::Error;

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
