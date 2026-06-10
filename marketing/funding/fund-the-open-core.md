# Fund the open core

> Publish as: GitHub Discussion (Announcements) + Discord #announcements +
> linked from leanctx.com/support. Publish-ready; no placeholders.

---

lean-ctx is one Rust binary that decides what your AI agents read, remembers
what they learn, and proves what they save — locally, with zero telemetry,
under Apache-2.0. Local use is free forever. That sentence is not marketing:
it is a frozen contract in `CONTRACTS.md`, and CI fails any change that
violates it.

Here is the honest part: **keeping that promise costs money.**

Every month, the open core needs:

- **Contract stability work.** 29 protocol contracts, the frozen ones
  SHA-256-locked in CI. SDKs for Python, TypeScript and Rust re-proven
  against a real engine build on every commit. This is the unglamorous
  engineering that makes "your setup will not break" true.
- **Security response.** PathJail, shell allowlist, secret redaction,
  OWASP-aligned injection screening — and somebody who drops everything
  when a CVE lands in a dependency.
- **Release engineering.** Reproducible builds for five targets, signed
  releases, a conformance gate that refuses to ship an engine the SDKs
  cannot speak to.

None of this produces a feature you can screenshot. All of it is why you can
put lean-ctx in front of your agents and forget about it.

## Three ways to fund it

**If lean-ctx saves you money** (the signed ledger will tell you exactly how
much — run `lean-ctx gain`):

1. **[Sponsor the open core](https://github.com/sponsors/yvgude)** — from
   $5/mo. Sponsorship buys recognition and access to the maintainers: README
   credit, office hours, weighted roadmap votes. It never buys features —
   the local-free invariant stays frozen for everyone.
2. **[Buy Team or Pro](https://leanctx.com/pricing/)** — if you want the
   hosted parts (team server, hosted index, cloud sync) managed for you.
   Subscription revenue funds the same open core.
3. **[Hire us](https://leanctx.com/services/)** — fixed-price onboarding,
   custom connectors, or a platform retainer. Every engine improvement from
   paid work lands upstream under Apache-2.0. You pay for prioritization
   and expertise, never for a fork.

## Where the money goes

We publish the receipts in both directions: the savings ledger proves what
lean-ctx saves you, and the [public metrics page](https://leanctx.com/metrics/)
shows project health. Sponsorship income funds maintainer time on the three
buckets above — contract stability first, security second, everything else
third.

If you cannot fund it: star the repo, report bugs precisely, answer one
question in Discord. That is funding too.

— Yves
