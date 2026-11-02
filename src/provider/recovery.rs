//! Provider-owned recovery timing; class meaning and send ceilings are not configurable.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecoveryPolicy {
    pub recovery_timeout_ms: u64,
    pub transient_cooldown_ms: u64,
    pub transient_multiplier: u32,
    pub transient_max_cooldown_ms: u64,
    pub rate_limit_cooldown_ms: u64,
    pub rate_limit_multiplier: u32,
    pub rate_limit_max_cooldown_ms: u64,
    pub reset_hint_floor_ms: u64,
    pub rate_limit_reset_max_ms: u64,
    pub quota_reset_max_ms: u64,
    pub quota_cooldown_ms: u64,
    pub auth_cooldown_ms: u64,
    pub permission_cooldown_ms: u64,
    pub failure_free_reset_ms: u64,
}
impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self {
            recovery_timeout_ms: 30_000,
            transient_cooldown_ms: 1_000,
            transient_multiplier: 2,
            transient_max_cooldown_ms: 60_000,
            rate_limit_cooldown_ms: 60_000,
            rate_limit_multiplier: 2,
            rate_limit_max_cooldown_ms: 1_800_000,
            reset_hint_floor_ms: 1_000,
            rate_limit_reset_max_ms: 3_600_000,
            quota_reset_max_ms: 604_800_000,
            quota_cooldown_ms: 1_800_000,
            auth_cooldown_ms: 1_800_000,
            permission_cooldown_ms: 1_800_000,
            failure_free_reset_ms: 1_800_000,
        }
    }
}
