//! File synchronization state definitions.

use serde::{Deserialize, Serialize};

/// Represents the synchronization state of a file.
///
/// These states align with the visual indicators shown to users
/// in their file explorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    /// File is fully synchronized and available locally.
    /// Visual indicator: Green checkmark ✓
    Synced,

    /// File exists only in cloud (placeholder locally).
    /// Visual indicator: Cloud icon ☁
    CloudOnly,

    /// File is currently being uploaded or downloaded.
    /// Visual indicator: Sync arrows ↻
    Syncing,

    /// File is queued for sync.
    /// Visual indicator: Clock icon ⏱
    #[default]
    Pending,

    /// Sync failed for this file.
    /// Visual indicator: Warning icon ⚠
    Error,

    /// File is excluded from sync.
    /// Visual indicator: Slash icon ⊘
    Excluded,

    /// Local changes pending upload (no connectivity).
    /// Visual indicator: Orange dot ●
    OfflineModified,

    /// File has conflicting versions.
    /// Visual indicator: Red exclamation ❗
    Conflict,
}

impl FileState {
    /// Returns a human-readable description of the state.
    pub fn description(&self) -> &'static str {
        match self {
            FileState::Synced => "File is fully synchronized",
            FileState::CloudOnly => "File is available in cloud only",
            FileState::Syncing => "File is being synchronized",
            FileState::Pending => "File is queued for synchronization",
            FileState::Error => "Synchronization failed",
            FileState::Excluded => "File is excluded from sync",
            FileState::OfflineModified => "Local changes pending upload",
            FileState::Conflict => "File has conflicting versions",
        }
    }

    /// Returns the icon/emoji representation for display.
    pub fn icon(&self) -> &'static str {
        match self {
            FileState::Synced => "✓",
            FileState::CloudOnly => "☁",
            FileState::Syncing => "↻",
            FileState::Pending => "⏱",
            FileState::Error => "⚠",
            FileState::Excluded => "⊘",
            FileState::OfflineModified => "●",
            FileState::Conflict => "❗",
        }
    }

    /// Returns whether the file content is available locally.
    pub fn is_available_locally(&self) -> bool {
        matches!(
            self,
            FileState::Synced | FileState::OfflineModified | FileState::Conflict
        )
    }

    /// Returns whether the file needs attention from the user.
    pub fn needs_attention(&self) -> bool {
        matches!(self, FileState::Error | FileState::Conflict)
    }

    /// Returns whether the file is currently in a transient sync state.
    pub fn is_syncing(&self) -> bool {
        matches!(self, FileState::Syncing | FileState::Pending)
    }
}

impl std::fmt::Display for FileState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_state_descriptions_are_non_empty() {
        let states = [
            FileState::Synced,
            FileState::CloudOnly,
            FileState::Syncing,
            FileState::Pending,
            FileState::Error,
            FileState::Excluded,
            FileState::OfflineModified,
            FileState::Conflict,
        ];

        for state in states {
            assert!(!state.description().is_empty());
            assert!(!state.icon().is_empty());
        }
    }

    #[test]
    fn synced_file_is_available_locally() {
        assert!(FileState::Synced.is_available_locally());
    }

    #[test]
    fn cloud_only_file_is_not_available_locally() {
        assert!(!FileState::CloudOnly.is_available_locally());
    }

    #[test]
    fn offline_modified_file_is_available_locally() {
        assert!(FileState::OfflineModified.is_available_locally());
    }

    #[test]
    fn conflict_file_is_available_locally() {
        assert!(FileState::Conflict.is_available_locally());
    }

    #[test]
    fn error_state_needs_attention() {
        assert!(FileState::Error.needs_attention());
    }

    #[test]
    fn conflict_state_needs_attention() {
        assert!(FileState::Conflict.needs_attention());
    }

    #[test]
    fn synced_state_does_not_need_attention() {
        assert!(!FileState::Synced.needs_attention());
    }

    #[test]
    fn syncing_state_is_syncing() {
        assert!(FileState::Syncing.is_syncing());
    }

    #[test]
    fn pending_state_is_syncing() {
        assert!(FileState::Pending.is_syncing());
    }

    #[test]
    fn synced_state_is_not_syncing() {
        assert!(!FileState::Synced.is_syncing());
    }

    #[test]
    fn default_state_is_pending() {
        assert_eq!(FileState::default(), FileState::Pending);
    }

    #[test]
    fn file_state_serializes_to_snake_case() {
        let state = FileState::CloudOnly;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"cloud_only\"");
    }

    #[test]
    fn file_state_deserializes_from_snake_case() {
        let state: FileState = serde_json::from_str("\"offline_modified\"").unwrap();
        assert_eq!(state, FileState::OfflineModified);
    }

    #[test]
    fn file_state_display_shows_description() {
        let state = FileState::Synced;
        assert_eq!(format!("{}", state), "File is fully synchronized");
    }
}
