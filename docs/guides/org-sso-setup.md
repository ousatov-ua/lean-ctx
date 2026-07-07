# Org Single Sign-On (OIDC) Setup

Let your team sign in to lean-ctx Cloud through your own identity provider
(Okta, Microsoft Entra ID, Google Workspace, or any OIDC-compliant OP).
Self-serve, no support ticket. Configure it once on the billing page, verify
your domain via DNS, and optionally require SSO for everyone in the org.

> **Plan:** Self-serve OIDC SSO is available on **Business** ($149/mo) and
> **Enterprise** plans. SAML SSO with SCIM provisioning is Enterprise-only.
> The org owner configures it; members just sign in.
>
> *Grandfather note:* orgs that configured OIDC while it was Team-gated
> (pre-GL #533) keep their existing SSO working. New SSO setup requires
> the `sso_oidc` entitlement (Business or Enterprise).

---

## How it works

1. You register a lean-ctx app in your IdP and paste the issuer, client ID and
   client secret into **Account → Billing → Single sign-on**.
2. You prove you own the email domain by adding one DNS-TXT record.
3. Members go to the login page, click **Continue with SSO**, enter their work
   email, and get redirected to your IdP. On return they have a normal
   lean-ctx session — no password, account auto-provisioned and added to your
   org (just-in-time).
4. Optional: flip **Require SSO** so password logins for your domain are
   refused. The org owner always keeps password access (break-glass) so a
   misconfigured IdP can never lock you out.

The redirect URI to register in every IdP is:

```
https://api.leanctx.com/api/auth/sso/callback
```

Required scopes: `openid email profile`. Response type: `code` (Authorization
Code flow with PKCE — lean-ctx adds PKCE automatically).

---

## Step 1 — Create the app in your IdP

### Okta

1. **Admin → Applications → Create App Integration**.
2. Sign-in method: **OIDC – OpenID Connect**. Application type: **Web
   Application**.
3. Sign-in redirect URI: `https://api.leanctx.com/api/auth/sso/callback`.
4. Assign the people/groups who should have access.
5. Copy **Client ID** and **Client secret**. Your **issuer** is
   `https://<your-org>.okta.com` (Okta → Security → API → Authorization Servers;
   use the `Issuer URI`).

### Microsoft Entra ID (Azure AD)

1. **Entra admin → App registrations → New registration**.
2. Redirect URI (type **Web**):
   `https://api.leanctx.com/api/auth/sso/callback`.
3. **Certificates & secrets → New client secret**, copy the value.
4. **Overview** → copy the **Application (client) ID**.
5. Issuer:
   `https://login.microsoftonline.com/<tenant-id>/v2.0`.

### Google Workspace

1. **Google Cloud Console → APIs & Services → Credentials → Create
   Credentials → OAuth client ID**.
2. Application type: **Web application**.
3. Authorized redirect URI:
   `https://api.leanctx.com/api/auth/sso/callback`.
4. Copy **Client ID** and **Client secret**.
5. Issuer: `https://accounts.google.com`.

> Any spec-compliant OIDC provider works. lean-ctx reads the issuer's
> `/.well-known/openid-configuration` for endpoints and JWKS, so you only need
> the issuer URL — not individual endpoint URLs.

---

## Step 2 — Configure lean-ctx

1. Sign in as the **org owner** and open **Account → Billing**.
2. In **Single sign-on (OIDC)**, fill in:
   - **Email domain** — e.g. `acme.com` (the domain of your members' work
     email).
   - **Issuer URL** — from step 1.
   - **Client ID** / **Client secret** — from step 1.
3. **Save configuration.** The secret is sealed (encrypted at rest) and never
   shown again; leave the field blank on later edits to keep the stored one.

---

## Step 3 — Verify your domain

After saving, lean-ctx shows a DNS-TXT record. Add it at your DNS provider:

| Field | Value |
|-------|-------|
| Type  | `TXT` |
| Name / Host | `_leanctx-sso.acme.com` |
| Value | `leanctx-sso-verify=<token shown in the dashboard>` |

Then click **Verify domain**. lean-ctx checks the record over DNS-over-HTTPS
(Cloudflare, then Google). DNS can take a few minutes to propagate — if it
isn't visible yet, wait and retry.

Domain verification is required before SSO accepts any login. It guarantees
that only the org which controls a domain can authenticate its addresses, and
that no two orgs can claim the same domain.

---

## Step 4 (optional) — Require SSO

Once the domain is verified, toggle **Require SSO for everyone in the org**.
While enabled:

- Password login and registration for your domain are refused.
- The **org owner is always exempt** (break-glass) — you can still sign in with
  your password if the IdP is down.

Turn it off any time to re-enable passwords.

---

## What members experience

1. Login page → **Continue with SSO** → enter work email.
2. Redirect to your IdP, authenticate (and MFA, if your IdP enforces it).
3. Back on lean-ctx, signed in. First-time users are created automatically
   (email pre-verified, no password) and added to your org.
4. They run `lean-ctx login` / configure the MCP key exactly as a
   password user would — the session is identical.

---

## Security model

- **Authorization Code + PKCE (S256)** on every flow, even with a confidential
  client secret.
- **ID-token verification**: JWKS signature, `iss`, `aud`, `exp`, and a
  per-flow `nonce`. Tokens signed with `HS*`/`none` are rejected outright
  (alg-confusion defense) — only RSA/PS/ECDSA are accepted.
- **Asserted email must be under your verified domain**, re-checked at the
  callback. `email_verified:false` from the IdP is rejected.
- **Client secret** is encrypted at rest (ChaCha20-Poly1305), decrypted only
  for the token exchange, never cached on the edge.
- **No API keys in URLs**: after a successful login the browser exchanges a
  single-use, 60-second handoff code for the session key.
- **Owner break-glass**: enforcement never applies to the owner's email.

Full protocol contract: `docs/contracts/org-sso-oidc-v1.md`.

---

## Troubleshooting

| Symptom | Cause / fix |
|---------|-------------|
| **Continue with SSO** says no IdP found | Domain not configured or not verified yet. Finish steps 2–3. |
| `sso_error=verify_failed` after IdP | ID token failed validation. Check the issuer URL is exact and the client ID matches the IdP app. |
| `sso_error=idp_denied` | The IdP rejected the user (not assigned to the app, or consent denied). Assign the user/group. |
| `sso_error=expired` | The login took longer than 10 minutes, or the handoff code expired. Just start again. |
| Domain won't verify | TXT record not yet propagated, wrong host (`_leanctx-sso.<domain>`), or wrong value. Re-check and retry — DoH reads can lag your DNS edit by minutes. |
| A user can't sign in but others can | Their email domain differs from the verified domain, or the IdP reports `email_verified:false`. |
| Owner locked out with SSO required | Owners are exempt by design — use your password. If a non-owner needs in, turn off **Require SSO** temporarily. |

Still stuck? `hello@leanctx.com` or the
[Discord community](https://discord.gg/pTHkG9Hew9).

