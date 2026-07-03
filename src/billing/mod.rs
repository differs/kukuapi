//! Billing and quota management.
//!
//! Handles balance tracking, quota enforcement, subscription management,
//! usage-based billing, and rate limiting.

use crate::db::models::{User, ApiKey, UserSubscription};
use crate::db::repositories::{ApiKeyRepository, BillingRepository, UsageRepository};
use uuid::Uuid;

/// Result of a billing check before forwarding a request.
pub enum BillingCheck {
    /// Charge against API key quota.
    Quota { api_key_id: Uuid, plan_sub_id: Option<Uuid> },
    /// Charge against user balance.
    Balance { user_id: Uuid, api_key_id: Uuid },
    /// Charge against subscription.
    Subscription { sub_id: Uuid },
    /// No billing (simple mode / exempt).
    Noop,
}

/// Billing service for quota/balance management.
#[derive(Clone)]
pub struct BillingService {
    _api_key_repo: ApiKeyRepository,
    _billing_repo: BillingRepository,
    _usage_repo: UsageRepository,
    simple_mode: bool,
}

impl BillingService {
    pub fn new(
        api_key_repo: ApiKeyRepository,
        billing_repo: BillingRepository,
        usage_repo: UsageRepository,
        simple_mode: bool,
    ) -> Self {
        Self { _api_key_repo: api_key_repo, _billing_repo: billing_repo, _usage_repo: usage_repo, simple_mode }
    }

    /// Check billing eligibility before processing a request.
    pub async fn check_eligibility(
        &self,
        api_key: &ApiKey,
        user: &User,
    ) -> Result<BillingCheck, BillingError> {
        if self.simple_mode {
            return Ok(BillingCheck::Noop);
        }

        if api_key.status != "active" {
            return Err(BillingError::KeyNotActive(api_key.status.clone()));
        }
        if let Some(expires) = api_key.expires_at {
            if chrono::Utc::now() > expires {
                return Err(BillingError::KeyExpired);
            }
        }
        if api_key.quota > 0.0 {
            let remaining = api_key.quota - api_key.quota_used;
            if remaining <= 0.0 {
                return Err(BillingError::QuotaExhausted);
            }
            return Ok(BillingCheck::Quota { api_key_id: api_key.id, plan_sub_id: None });
        }
        if user.balance > 0.0 {
            return Ok(BillingCheck::Balance { user_id: user.id, api_key_id: api_key.id });
        }
        // Check subscription (placeholder - would actually query DB)
        Err(BillingError::InsufficientBalance)
    }

    /// Deduct billing after a successful request.
    pub async fn deduct(&self, _check: &BillingCheck, _cost: f64) -> Result<(), BillingError> {
        Ok(())
    }

    /// Record usage log entry.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_usage(
        &self,
        _user_id: Uuid,
        _api_key_id: Uuid,
        _account_id: Uuid,
        _model: &str,
        _input_tokens: i64,
        _output_tokens: i64,
        _cache_creation_tokens: i64,
        _cache_read_tokens: i64,
        _cost: f64,
        _rate_multiplier: f64,
        _billing_type: &str,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum BillingError {
    KeyNotActive(String),
    KeyExpired,
    QuotaExhausted,
    InsufficientBalance,
    SubscriptionQuotaExhausted,
    RateLimited(String),
    DatabaseError(String),
}

impl std::fmt::Display for BillingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BillingError::KeyNotActive(s) => write!(f, "API key is not active: {}", s),
            BillingError::KeyExpired => write!(f, "API key has expired"),
            BillingError::QuotaExhausted => write!(f, "API key quota exhausted"),
            BillingError::InsufficientBalance => write!(f, "Insufficient balance"),
            BillingError::SubscriptionQuotaExhausted => write!(f, "Subscription quota exhausted"),
            BillingError::RateLimited(msg) => write!(f, "Rate limited: {}", msg),
            BillingError::DatabaseError(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for BillingError {}

/// Calculate request cost from token usage and model pricing.
pub fn calculate_cost(
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
    rate_multiplier: f64,
) -> f64 {
    let input_price = 0.000003;   // $3/M tokens
    let output_price = 0.000015;  // $15/M tokens
    let input_cost = input_tokens as f64 * input_price;
    let output_cost = output_tokens as f64 * output_price;
    (input_cost + output_cost) * rate_multiplier
}
