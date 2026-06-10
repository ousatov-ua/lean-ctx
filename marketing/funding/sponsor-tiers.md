# Sponsor Tiers — GitHub Sponsors + Open Collective (GL #466)

> Copy-paste source for the GitHub Sponsors dashboard and the Open Collective
> page. One human action required: create the tiers in both UIs (≈20 min).
> Design rule, non-negotiable: **sponsorship buys recognition and access to
> people, never features.** The local-free invariant is a frozen contract.

## Why these tiers

lean-ctx is Apache-2.0 and free locally, forever — CI-enforced. Sponsorship
funds maintenance: CVE response, dependency updates, contract stability,
release engineering. Sponsors get recognition and time with the maintainers,
not gated features. Anyone who wants managed infrastructure buys
[Team/Pro](https://leanctx.com/pricing/); anyone who wants hands-on help buys
[Services](https://leanctx.com/services/). Sponsoring is for people and
companies that want the open core to stay healthy.

## Monthly tiers (GitHub Sponsors + Open Collective, identical)

### $5/mo — Supporter
> You keep the lights on.

- Sponsor badge on your GitHub profile
- Name on the [supporters wall](https://leanctx.com/support/) (opt-in)
- Our genuine gratitude — this tier funds CI minutes

### $25/mo — Backer
> You fund a dependency audit every month.

Everything in Supporter, plus:
- Name + link in `README.md` backers section
- Early access to release-candidate announcements (Discord role)

### $100/mo — Sponsor
> You fund contract stability: frozen surfaces, conformance suites, CVE response.

Everything in Backer, plus:
- Logo + link in `README.md` sponsors section and on leanctx.com/support
- **Roadmap vote:** one weighted vote per public roadmap cycle (we publish the
  tally; votes prioritize, they cannot add proprietary features)
- Invitation to the monthly **office hour** (group call with the maintainer)

### $500/mo — Corporate Sponsor
> Your engineers run lean-ctx in production and you want it maintained like
> infrastructure.

Everything in Sponsor, plus:
- Large logo at the top of the README sponsors section + homepage footer
- **Two named engineers** in the priority-triage lane on GitHub issues
  (triage priority, not private fixes — everything still lands upstream)
- Quarterly 60-min roadmap session with the maintainer

## One-time tiers

- **$10 — Coffee:** thanks in the next release notes (opt-in)
- **$250 — Feature bounty boost:** attach to any open issue; shown as a
  bounty label. Does not guarantee implementation — it signals demand.

## What sponsorship explicitly does NOT buy

- No feature gates ever — local functionality is identical for everyone
- No private forks, no proprietary builds
- No SLA (that is the [Platform Retainer](https://leanctx.com/services/))
- No influence over security policy or the stability contract

## Setup checklist (human, ≈20 min)

- [ ] GitHub Sponsors: create the four monthly + two one-time tiers above
      (dashboard → Sponsor tiers → copy the text verbatim)
- [ ] Open Collective: mirror the same tiers (collective `lean-ctx`)
- [ ] Add `open_collective: lean-ctx` + `buy_me_a_coffee: yvgude` to
      `.github/FUNDING.yml` (PR ready — see repo)
- [ ] Publish `marketing/funding/fund-the-open-core.md` as a GitHub
      Discussion (Announcements) + Discord #announcements + link from
      the supporters wall
