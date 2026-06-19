//! Success-fee computation for outcome-based pricing (GL #669, EPIC #671).
//!
//! Turns a **verified** savings figure (from the Ed25519-signed savings batch,
//! via [`Usage`](crate::core::billing::metering::Usage)) into the agreed
//! enterprise success fee:
//!
//! ```text
//! base = fixed_floor + take_rate * (saved_usd * cache_haircut)
//! cap  = invoice_cap_pct * provider_delta_usd
//! fee  = min(base, cap)        // never exceeds the agreed share of real spend
//! ```
//!
//! The **cache-haircut** discounts savings that came from cache hits (cheaper to
//! the customer than fresh reads), and the **invoice-cap** guarantees the fee
//! never exceeds a pre-agreed fraction of the customer's *actual* provider-bill
//! delta — the over-billing guard the enterprise contract promises.
//!
//! The four parameters have **no defaults**: they are commercial terms that must
//! be set explicitly per customer ([`FeeParams::from_config`] fails closed when
//! any is missing), so lean-ctx never invents a price.

use serde::Serialize;

use crate::core::config::SuccessFeeConfig;

/// Validated commercial fee parameters. Constructed only via
/// [`FeeParams::from_config`], which enforces presence and ranges — so a
/// `FeeParams` value is always a usable, in-range set of terms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeeParams {
    /// Share of (haircut-adjusted) verified savings charged as the fee, `0..=1`.
    pub take_rate: f64,
    /// Fixed component added before the cap (USD), `>= 0`.
    pub fixed_floor: f64,
    /// Multiplier applied to verified savings to discount cache-sourced savings, `0..=1`.
    pub cache_haircut: f64,
    /// The fee may never exceed this fraction of the customer-provided provider
    /// delta, `0..=1`.
    pub invoice_cap_pct: f64,
}

impl FeeParams {
    /// Build validated params from config, failing closed when any commercial
    /// term is unset (no defaults) or out of range. The error names every
    /// offending key so the operator can fix the config in one pass.
    pub fn from_config(cfg: &SuccessFeeConfig) -> Result<Self, String> {
        let mut missing = Vec::new();
        let take_rate = require(cfg.take_rate, "success_fee.take_rate", &mut missing);
        let fixed_floor = require(cfg.fixed_floor, "success_fee.fixed_floor", &mut missing);
        let cache_haircut = require(cfg.cache_haircut, "success_fee.cache_haircut", &mut missing);
        let invoice_cap_pct = require(
            cfg.invoice_cap_pct,
            "success_fee.invoice_cap_pct",
            &mut missing,
        );

        if !missing.is_empty() {
            return Err(format!(
                "success-fee terms are not configured (no defaults): {}. \
                 Set them with `lean-ctx config set <key> <value>`.",
                missing.join(", ")
            ));
        }

        let params = Self {
            take_rate,
            fixed_floor,
            cache_haircut,
            invoice_cap_pct,
        };
        params.validate()?;
        Ok(params)
    }

    fn validate(&self) -> Result<(), String> {
        check_fraction(self.take_rate, "success_fee.take_rate")?;
        check_fraction(self.cache_haircut, "success_fee.cache_haircut")?;
        check_fraction(self.invoice_cap_pct, "success_fee.invoice_cap_pct")?;
        if !self.fixed_floor.is_finite() || self.fixed_floor < 0.0 {
            return Err("success_fee.fixed_floor must be >= 0".to_string());
        }
        Ok(())
    }

    /// Compute the fee for one billing period from a verified `saved_usd` and the
    /// customer-provided `provider_delta_usd` (their actual provider-bill change).
    #[must_use]
    pub fn compute(&self, saved_usd: f64, provider_delta_usd: f64) -> FeeBreakdown {
        let saved = saved_usd.max(0.0);
        let provider_delta = provider_delta_usd.max(0.0);

        let adjusted_savings = saved * self.cache_haircut;
        let base = self.fixed_floor + self.take_rate * adjusted_savings;
        let cap = self.invoice_cap_pct * provider_delta;

        let capped = base > cap;
        let fee = if capped { cap } else { base };
        // Round to whole cents; Stripe bills integer minor units.
        let amount_cents = (fee.max(0.0) * 100.0).round() as i64;

        FeeBreakdown {
            saved_usd: saved,
            provider_delta_usd: provider_delta,
            adjusted_savings_usd: adjusted_savings,
            base_fee_usd: base,
            cap_usd: cap,
            fee_usd: fee.max(0.0),
            amount_cents: amount_cents.max(0),
            capped,
        }
    }
}

/// The full, auditable result of a fee computation (every intermediate value is
/// exposed so the invoice and the `--json` output are self-explaining).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeeBreakdown {
    pub saved_usd: f64,
    pub provider_delta_usd: f64,
    pub adjusted_savings_usd: f64,
    pub base_fee_usd: f64,
    pub cap_usd: f64,
    pub fee_usd: f64,
    /// The billable amount in integer minor units (cents) for Stripe.
    pub amount_cents: i64,
    /// Whether the invoice-cap clamped the fee below the formula base.
    pub capped: bool,
}

impl FeeBreakdown {
    /// Whether there is anything to bill (a zero fee never creates an invoice).
    #[must_use]
    pub fn is_billable_amount(&self) -> bool {
        self.amount_cents > 0
    }
}

fn require(value: Option<f64>, key: &str, missing: &mut Vec<String>) -> f64 {
    if let Some(v) = value {
        v
    } else {
        missing.push(key.to_string());
        0.0
    }
}

fn check_fraction(v: f64, key: &str) -> Result<(), String> {
    if !v.is_finite() || !(0.0..=1.0).contains(&v) {
        return Err(format!("{key} must be a fraction in 0.0..=1.0 (got {v})"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(tr: f64, ff: f64, ch: f64, cap: f64) -> SuccessFeeConfig {
        SuccessFeeConfig {
            take_rate: Some(tr),
            fixed_floor: Some(ff),
            cache_haircut: Some(ch),
            invoice_cap_pct: Some(cap),
            currency: None,
            stripe_customer: None,
        }
    }

    #[test]
    fn missing_terms_fail_closed_and_are_all_named() {
        let err = FeeParams::from_config(&SuccessFeeConfig::default()).unwrap_err();
        assert!(err.contains("success_fee.take_rate"));
        assert!(err.contains("success_fee.fixed_floor"));
        assert!(err.contains("success_fee.cache_haircut"));
        assert!(err.contains("success_fee.invoice_cap_pct"));
    }

    #[test]
    fn out_of_range_rejected() {
        assert!(FeeParams::from_config(&cfg(1.5, 0.0, 1.0, 1.0)).is_err());
        assert!(FeeParams::from_config(&cfg(0.2, -1.0, 1.0, 1.0)).is_err());
        assert!(FeeParams::from_config(&cfg(0.2, 0.0, 2.0, 1.0)).is_err());
    }

    #[test]
    fn base_formula_without_cap_binding() {
        // floor 1000 + 0.2 * (10000 * 0.8) = 1000 + 1600 = 2600
        // cap = 0.5 * 100000 = 50000 (not binding)
        let p = FeeParams::from_config(&cfg(0.2, 1000.0, 0.8, 0.5)).unwrap();
        let b = p.compute(10_000.0, 100_000.0);
        assert!((b.base_fee_usd - 2600.0).abs() < 1e-9);
        assert!((b.fee_usd - 2600.0).abs() < 1e-9);
        assert!(!b.capped);
        assert_eq!(b.amount_cents, 260_000);
    }

    #[test]
    fn cap_binds_and_clamps_fee() {
        // base = 0 + 1.0 * (100000 * 1.0) = 100000
        // cap  = 0.1 * 200000 = 20000  -> fee clamped to 20000
        let p = FeeParams::from_config(&cfg(1.0, 0.0, 1.0, 0.1)).unwrap();
        let b = p.compute(100_000.0, 200_000.0);
        assert!((b.base_fee_usd - 100_000.0).abs() < 1e-6);
        assert!((b.cap_usd - 20_000.0).abs() < 1e-6);
        assert!((b.fee_usd - 20_000.0).abs() < 1e-6);
        assert!(b.capped);
        assert_eq!(b.amount_cents, 2_000_000);
    }

    #[test]
    fn zero_provider_delta_caps_fee_to_zero() {
        let p = FeeParams::from_config(&cfg(0.2, 1000.0, 1.0, 0.5)).unwrap();
        let b = p.compute(10_000.0, 0.0);
        assert_eq!(b.fee_usd, 0.0);
        assert_eq!(b.amount_cents, 0);
        assert!(!b.is_billable_amount());
    }

    #[test]
    fn negative_inputs_are_floored_to_zero() {
        let p = FeeParams::from_config(&cfg(0.2, 0.0, 1.0, 1.0)).unwrap();
        let b = p.compute(-5.0, -5.0);
        assert_eq!(b.amount_cents, 0);
    }

    #[test]
    fn cents_rounding_is_half_up() {
        // base = 0.005 floor, no savings -> 0.005 USD -> 0.5 cents -> rounds to 1
        let p = FeeParams::from_config(&cfg(0.0, 0.005, 1.0, 1.0)).unwrap();
        let b = p.compute(0.0, 1.0);
        assert_eq!(b.amount_cents, 1);
    }
}
