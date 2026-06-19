//! `lean-ctx billing` — read-only commercial-plane reporting (EPIC 13.6):
//! plans, entitlements, metered usage. Never gates the local plane.

use super::savings::savings_agent_id;
use crate::core;
use crate::core::billing::stripe_invoice::{InvoiceItemRequest, StripeClient};

/// `lean-ctx billing <plans|entitlements|usage>` — the commercial-plane billing
/// substrate (EPIC 13.6). All subcommands are **informational and read-only**:
/// they describe plans/entitlements and meter local savings. The local plane is
/// never gated — there are no entitlement checks here, only reporting.
pub(in crate::cli::dispatch) fn cmd_billing(rest: &[String]) {
    let action = rest.first().map_or("usage", String::as_str);
    let json = rest.iter().any(|a| a == "--json");
    match action {
        "status" => cmd_billing_status(json),
        "plans" => cmd_billing_plans(json),
        "entitlements" => cmd_billing_entitlements(rest.get(1).map(String::as_str), json),
        "usage" => cmd_billing_usage(json),
        "invoice" => cmd_billing_invoice(rest, json),
        other => {
            eprintln!(
                "unknown billing action '{other}'. Use: status | plans | entitlements <plan> | usage | invoice [--json]"
            );
            std::process::exit(1);
        }
    }
}

/// Reads `--key=value` style flags.
fn flag(args: &[String], prefix: &str) -> Option<String> {
    args.iter()
        .find_map(|a| a.strip_prefix(prefix))
        .map(str::to_string)
}

/// `lean-ctx billing status [--json]` — the at-a-glance commercial state for this
/// machine: the effective plan (with offline-grace provenance), the hosted
/// entitlements it grants, and the local ROI headline. Read-only; it best-effort
/// refreshes the plan from the backend and falls back to the cached-with-grace
/// plan when offline. Never gates anything local.
fn cmd_billing_status(json: bool) {
    use crate::cloud_client::PlanSource;
    let eff = crate::cloud_client::refresh_effective_plan();
    let logged_in = crate::cloud_client::is_logged_in();
    let e = eff.plan.entitlements();
    let roi = core::savings_ledger::roi_report(&savings_agent_id());

    if json {
        let payload = serde_json::json!({
            "plan": eff.plan.as_str(),
            "source": plan_source_label(eff.source),
            "verified_at": eff.verified_at,
            "grace_days": eff.grace_days,
            "logged_in": logged_in,
            "entitlements": e,
            "roi": {
                "net_saved_tokens": roi.net_saved_tokens,
                "saved_usd": roi.saved_usd,
                "total_events": roi.total_events,
                "chain_valid": roi.chain_valid,
                "signed": roi.signed,
            }
        });
        print_json_or_die(&payload, "billing status");
        return;
    }

    println!("lean-ctx billing status\n");
    println!(
        "  Plan:         {}  ({})",
        eff.plan.as_str(),
        plan_source_detail(&eff)
    );
    println!(
        "  Account:      {}",
        if logged_in {
            "logged in"
        } else {
            "not logged in (Free)"
        }
    );
    println!("  cloud_sync:   {}", yesno(e.cloud_sync));
    println!("  seats:        {}", quota(e.seats));
    println!(
        "  private_registry: {}   sso_oidc: {}   sso_scim: {}",
        e.private_registry, e.sso_oidc, e.sso_scim
    );
    println!();
    println!(
        "  ROI:          {} net tokens · ${:.2}  ({}, {})",
        roi.net_saved_tokens,
        roi.saved_usd,
        if roi.chain_valid {
            "chain valid"
        } else {
            "chain BROKEN"
        },
        if roi.signed { "signed" } else { "unsigned" }
    );
    println!("  Full report:  lean-ctx roi");
    println!();
    match eff.source {
        PlanSource::Expired => {
            println!("  ! Cached plan expired — reconnect: lean-ctx login, then lean-ctx sync");
        }
        PlanSource::License => {
            println!("  Source:       offline Enterprise license — lean-ctx license status");
        }
        PlanSource::None if !logged_in => {
            println!(
                "  Upgrade:      lean-ctx cloud upgrade   (Pro: hosted sync · Team: shared ROI rollup)"
            );
        }
        _ => println!("  Manage:       lean-ctx cloud upgrade"),
    }
}

/// Stable wire label for a [`crate::cloud_client::PlanSource`].
fn plan_source_label(source: crate::cloud_client::PlanSource) -> &'static str {
    use crate::cloud_client::PlanSource;
    match source {
        PlanSource::Live => "live",
        PlanSource::Cached => "cached",
        PlanSource::Expired => "expired",
        PlanSource::None => "none",
        PlanSource::License => "license",
    }
}

/// Human provenance line: how fresh the plan is and how long the offline grace
/// keeps it valid.
fn plan_source_detail(eff: &crate::cloud_client::EffectivePlan) -> String {
    use crate::cloud_client::PlanSource;
    match eff.source {
        PlanSource::Live => "live".to_string(),
        PlanSource::Cached => match eff.verified_at {
            Some(v) => {
                let age_days = (chrono::Utc::now().timestamp() - v).max(0) / 86_400;
                let remaining = (eff.grace_days - age_days).max(0);
                format!("cached — verified {age_days}d ago, valid {remaining}d more")
            }
            None => "cached".to_string(),
        },
        PlanSource::Expired => "cached plan expired".to_string(),
        PlanSource::None => "no account".to_string(),
        PlanSource::License => "offline license".to_string(),
    }
}

fn yesno(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

fn cmd_billing_plans(json: bool) {
    let plans: Vec<core::billing::Entitlements> = core::billing::Plan::all()
        .iter()
        .map(|p| p.entitlements())
        .collect();
    if json {
        print_json_or_die(&plans, "plans");
        return;
    }
    println!("lean-ctx plans (commercial plane — additive, never gates local):\n");
    for e in &plans {
        println!("  {} — seats: {}", e.plan.as_str(), quota(e.seats));
        println!(
            "    hosted_index_mb: {}  connectors: {}  private_registry: {}",
            quota(e.hosted_index_mb),
            quota(e.managed_connectors),
            e.private_registry
        );
        println!(
            "    sso_oidc: {}  sso_scim: {}  audit_retention_days: {}  revenue_share: {}  supporter: {}",
            e.sso_oidc, e.sso_scim, e.audit_retention_days, e.revenue_share, e.supporter
        );
    }
    println!("\nThe Personal plane (local engine) is free + ungated regardless of plan.");
}

fn cmd_billing_entitlements(plan_arg: Option<&str>, json: bool) {
    let plan = core::billing::Plan::parse(plan_arg.unwrap_or("free"));
    let e = plan.entitlements();
    if json {
        print_json_or_die(&e, "entitlements");
        return;
    }
    println!("Entitlements for plan '{}':", plan.as_str());
    println!("  seats:                {}", quota(e.seats));
    println!("  hosted_index_mb:      {}", quota(e.hosted_index_mb));
    println!("  managed_connectors:   {}", quota(e.managed_connectors));
    println!("  private_registry:     {}", e.private_registry);
    println!("  sso_oidc:             {}", e.sso_oidc);
    println!("  sso_scim:             {}", e.sso_scim);
    println!("  audit_retention_days: {}", e.audit_retention_days);
    println!("  revenue_share:        {}", e.revenue_share);
    println!("  supporter:            {}", e.supporter);
}

fn cmd_billing_usage(json: bool) {
    let agent_id = savings_agent_id();
    let usage = core::billing::metered_usage(&agent_id);
    if json {
        print_json_or_die(&usage, "usage");
        return;
    }
    println!("{}", usage.headline());
    println!();
    println!("  Period:        {}", usage.period);
    println!("  Metered events: {}", usage.metered_events);
    println!("  Net tokens:    {}", usage.net_saved_tokens);
    println!("  Saved USD:     ${:.4}", usage.saved_usd);
    println!(
        "  Billable:      {}",
        if usage.is_billable() {
            "yes (signed + chain intact)"
        } else {
            "no (requires a signed, intact ledger)"
        }
    );
    println!("  Provenance:    {}", usage.last_entry_hash);
}

/// `lean-ctx billing invoice --provider-delta-usd=N [--customer=cus_…] [--period=LABEL]
/// [--create-invoice [--finalize]] [--dry-run] [--json]` — turn the **verified**
/// signed savings into a Stripe success-fee invoice item (GL #669).
///
/// Fail-closed for *billing*: an unsigned or broken ledger never produces an
/// invoice (but never blocks the local experience). The four fee terms have no
/// defaults — the command refuses to invent a price. Stripe TEST mode only.
fn cmd_billing_invoice(rest: &[String], json: bool) {
    let dry_run = rest.iter().any(|a| a == "--dry-run");
    let create_invoice = rest.iter().any(|a| a == "--create-invoice");
    let finalize = rest.iter().any(|a| a == "--finalize");

    let cfg = core::config::Config::load();

    // 1. Commercial terms (no defaults → fail closed when unset).
    let params = match core::billing::FeeParams::from_config(&cfg.success_fee) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    // 2. The invoice cap applies to the customer's actual provider-bill delta,
    //    which only the customer knows — so it is a required input.
    let Some(delta_str) = flag(rest, "--provider-delta-usd=") else {
        eprintln!(
            "--provider-delta-usd=<USD> is required: the customer-provided provider-bill \
             delta the invoice cap (success_fee.invoice_cap_pct) applies to."
        );
        std::process::exit(1);
    };
    let provider_delta = match delta_str.trim().parse::<f64>() {
        Ok(v) if v.is_finite() && v >= 0.0 => v,
        _ => {
            eprintln!("--provider-delta-usd must be a non-negative number (got '{delta_str}')");
            std::process::exit(1);
        }
    };

    // 3. Verified usage. Only a signed + intact chain is billable.
    let agent_id = savings_agent_id();
    let usage = core::billing::metered_usage(&agent_id);
    if !usage.is_billable() {
        let reason = format!(
            "ledger is {} / {}",
            if usage.signed { "signed" } else { "UNSIGNED" },
            if usage.chain_valid {
                "chain intact"
            } else {
                "chain BROKEN"
            },
        );
        if json {
            let payload = serde_json::json!({
                "billable": false,
                "reason": reason,
                "saved_usd": usage.saved_usd,
                "signed": usage.signed,
                "chain_valid": usage.chain_valid,
            });
            print_json_or_die(&payload, "billing invoice");
        } else {
            eprintln!("Not billable ({reason}). No invoice created (billing fails closed).");
        }
        std::process::exit(2);
    }

    // 4. Compute the fee.
    let breakdown = params.compute(usage.saved_usd, provider_delta);
    let currency = cfg
        .success_fee
        .currency
        .clone()
        .unwrap_or_else(|| "usd".to_string());
    let customer = flag(rest, "--customer=").or_else(|| cfg.success_fee.stripe_customer.clone());
    let period = flag(rest, "--period=").unwrap_or_else(|| usage.period.clone());
    let head_short: String = usage.last_entry_hash.chars().take(16).collect();
    let idem = sanitize_idem(&format!(
        "leanctx-fee-{}-{}",
        customer.as_deref().unwrap_or("nocustomer"),
        period
    ));
    let description = format!(
        "lean-ctx success fee — period {period} (verified savings ${:.2})",
        usage.saved_usd
    );

    let metadata = vec![
        ("lean_ctx_period".to_string(), period.clone()),
        (
            "lean_ctx_ledger_head".to_string(),
            usage.last_entry_hash.clone(),
        ),
        ("lean_ctx_agent".to_string(), usage.agent_id.clone()),
        (
            "saved_usd".to_string(),
            format!("{:.6}", breakdown.saved_usd),
        ),
        (
            "provider_delta_usd".to_string(),
            format!("{:.6}", breakdown.provider_delta_usd),
        ),
        ("take_rate".to_string(), params.take_rate.to_string()),
        (
            "fixed_floor_usd".to_string(),
            params.fixed_floor.to_string(),
        ),
        (
            "cache_haircut".to_string(),
            params.cache_haircut.to_string(),
        ),
        (
            "invoice_cap_pct".to_string(),
            params.invoice_cap_pct.to_string(),
        ),
        (
            "base_fee_usd".to_string(),
            format!("{:.6}", breakdown.base_fee_usd),
        ),
        ("cap_usd".to_string(), format!("{:.6}", breakdown.cap_usd)),
        ("capped".to_string(), breakdown.capped.to_string()),
    ];

    // Nothing to bill → never create an empty invoice.
    if !breakdown.is_billable_amount() {
        if json {
            let payload = serde_json::json!({
                "billable": true,
                "created": false,
                "reason": "computed fee rounds to $0.00",
                "currency": currency,
                "fee": breakdown,
            });
            print_json_or_die(&payload, "billing invoice");
        } else {
            print_invoice_preview(
                &breakdown,
                &currency,
                &period,
                &head_short,
                customer.as_deref(),
            );
            println!("\n  Computed fee is $0.00 → no invoice created.");
        }
        return;
    }

    if dry_run {
        if json {
            let payload = serde_json::json!({
                "billable": true,
                "created": false,
                "dry_run": true,
                "currency": currency,
                "customer": customer,
                "period": period,
                "idempotency_key": idem,
                "fee": breakdown,
            });
            print_json_or_die(&payload, "billing invoice");
        } else {
            print_invoice_preview(
                &breakdown,
                &currency,
                &period,
                &head_short,
                customer.as_deref(),
            );
            println!("\n  Dry run — no Stripe call made. Re-run without --dry-run to create it.");
        }
        return;
    }

    // 5. Real (test-mode) Stripe call. Customer is required now.
    let Some(customer) = customer else {
        eprintln!(
            "No Stripe customer: pass --customer=cus_… or set success_fee.stripe_customer in config."
        );
        std::process::exit(1);
    };
    let client = match StripeClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let item_req = InvoiceItemRequest {
        customer: customer.clone(),
        amount_cents: breakdown.amount_cents,
        currency: currency.clone(),
        description,
        metadata,
    };
    let item = match client.create_invoice_item(&item_req, &idem) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    // Optionally draft (and, with --finalize, advance) an invoice that pulls the
    // pending item. Idempotent per period via a distinct key suffix.
    let invoice = if create_invoice {
        match client.create_invoice(&customer, &format!("{idem}-inv"), finalize) {
            Ok(o) => Some(o),
            Err(e) => {
                eprintln!(
                    "invoice item created ({}) but drafting the invoice failed: {e}",
                    item.id
                );
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    if json {
        let payload = serde_json::json!({
            "billable": true,
            "created": true,
            "mode": "test",
            "currency": currency,
            "customer": customer,
            "period": period,
            "idempotency_key": idem,
            "fee": breakdown,
            "invoice_item": item,
            "invoice": invoice,
        });
        print_json_or_die(&payload, "billing invoice");
        return;
    }

    print_invoice_preview(&breakdown, &currency, &period, &head_short, Some(&customer));
    println!("\n  Stripe (TEST): invoice item {} created.", item.id);
    if let Some(inv) = invoice {
        println!(
            "  Stripe (TEST): invoice {} ({}).",
            inv.id,
            inv.status.as_deref().unwrap_or("draft")
        );
    } else {
        println!("  (pending on the customer — add --create-invoice to draft an invoice now.)");
    }
}

/// Print the fee breakdown in a stable, human-readable block.
fn print_invoice_preview(
    b: &core::billing::FeeBreakdown,
    currency: &str,
    period: &str,
    head_short: &str,
    customer: Option<&str>,
) {
    println!("lean-ctx success-fee invoice (Stripe TEST mode)\n");
    println!("  Customer:      {}", customer.unwrap_or("<unset>"));
    println!("  Period:        {period}");
    println!("  Ledger head:   {head_short}");
    println!("  Verified saved: ${:.2}", b.saved_usd);
    println!("  Provider delta: ${:.2}", b.provider_delta_usd);
    println!(
        "  Base fee:      ${:.2}  (floor + rate × saved × haircut)",
        b.base_fee_usd
    );
    println!(
        "  Cap:           ${:.2}  (cap_pct × provider delta)",
        b.cap_usd
    );
    println!(
        "  Fee:           ${:.2} {}{}",
        b.fee_usd,
        currency.to_uppercase(),
        if b.capped { "  (capped)" } else { "" }
    );
    println!("  Amount:        {} cents", b.amount_cents);
}

/// Stripe idempotency keys must be ASCII and bounded; map anything else to `-`.
fn sanitize_idem(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(200)
        .collect()
}

/// Render a quota: [`core::billing::plans::UNBOUNDED`] → "unlimited", else the
/// number (a plain `0` means *none*).
fn quota(n: u32) -> String {
    if n == core::billing::plans::UNBOUNDED {
        "unlimited".to_string()
    } else {
        n.to_string()
    }
}

fn print_json_or_die<T: serde::Serialize>(value: &T, what: &str) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("{what} serialization failed: {e}");
            std::process::exit(1);
        }
    }
}
