# sietesodas log

## 2026-11-05 — Core idea session

### Summary
First session. Established the core idea of sietesodas: an anonymous, Craigslist-style local classifieds service with no registered users. First audience is amateur musicians selling gear, designed to generalize later. No code written yet; this session was product design only.

### Main points
- Publishing is as easy as possible: fill in the post, give a contact method of your choice, submit.
- Browsing is search and category navigation only. Nothing is recommended or offered by default.
- Local scope. The geo approach will be borrowed from the user's `qhlv` repo.
- Anonymity creates three problems to handle: managing your own post without an account, spam, and exposing contact details.

### Decisions
- **No accounts.** Post ownership is a secret edit/delete token in a URL, shown once. Losing the link means losing control of the post.
- **Navigation.** Category browsing, search bar, and posting buttons. No post recommendations.
- **Spam defense.** Proof-of-work on publish (self-hostable, e.g. Altcha or mCaptcha), paired with per-IP rate limiting.
- **Expiry.** 20 days base. A generated contact adds 10 days. Hard cap of 30 days from publish. Exact extension semantics still to confirm (see todos).
- **Contact relay: masked alias address.** Each post gets its own address (e.g. `abc123@sietesodas.com`). Mail sent to it is forwarded to the seller's real email, and replies are rewritten so neither side sees the other's address. Internal on-site DMs are a possible later option, not part of the first version.
- **Photos.** Deferred to later.
- **Name.** sietesodas.
- **Email cost.** Prices from memory, verify before committing: Resend free tier about 3,000/month, Postmark about $15/month, Amazon SES about $0.10 per 1,000, self-hosted Postal free but has deliverability risk. A domain and SPF/DKIM/DMARC records are required either way. The alias relay needs inbound mail handling, which narrows the provider choice (see todos).

### qhlv review (`/Users/alex/Documents/qhlv`)
Local repo, `git init` done but no commits, so not pushed. Reference clone of the original app is in `./hlv` (github.com/alpad5/hlv). Read `PLAN.md`, `QHLV-LOG.md`, `space/geo.rs`, `space/mod.rs`. Did not read `store.rs` or `config.rs`.

- **Fit.** qhlv principles match sietesodas: anonymous, no history, TTL with inactivity timer plus hard cap, an activity resets the timer, proximity-scoped. A post maps to an object, an inbound alias email maps to an activity, the 30-day cap maps to the hard cap.
- **Geo.** `GeoSpace` snaps coordinates to a ~100 m grid plus Gaussian noise, and queries by radius in km via Redis `GEOSEARCH`. Pluggable through the `Space` trait.
- **Gaps for sietesodas.**
  1. Category and text search: `GeoSpace` only does radius queries; facet filters are planned but empty. Needs facet support in the engine or a separate index such as RediSearch.
  2. Ownership: qhlv has no identity, so no edit/delete endpoint. The secret token must be added in the engine or app layer.
  3. WebSocket real-time push is probably unnecessary for classifieds. Optional.
  4. Alias email is new work either way. A provider webhook would call the add-activity path.
  5. Engine is unfinished: `cargo check` fails with 2 small compile errors (listed in `QHLV-LOG.md`), `apps/hlv` consumer and parity check not done.
- **Unverified.** Whether an activity resets the inactivity timer to the full window or can add a fixed +10 days. Check `store.rs`.

### Todos
1. **Decide: build sietesodas on top of qhlv, or standalone borrowing only the geo code.** Recommendation: on qhlv. Reuses expiry, geo and Redis, and makes sietesodas the engine's second real consumer. Cost: fix the 2 compile errors first, and add facet filtering before search works. Standalone is faster to a first version but duplicates expiry and geo logic.
2. ~~**Confirm expiry semantics.**~~ Resolved 2026-11-06: +10 days per contact, capped at 30 days from publish. With aliases, "contact generated" means an inbound email from a buyer to the post's alias. Note that 20 + 10 already reaches the cap, so a single contact maxes out the lifetime.
3. **Pick an email provider that supports inbound mail plus outbound sending and reply rewriting.** Candidates to compare: Postmark inbound, Amazon SES receiving, Mailgun routes, Cloudflare Email Routing/Workers.
4. **Design the alias relay.** Points to settle:
   - Alias format and how it maps to a post and the seller's email.
   - Reply rewriting: how a seller's reply reaches the buyer without exposing the seller's address.
   - Loop and spam protection on inbound mail, since the alias is a public address that forwards to a real inbox.
   - Data retention: delete the stored seller email and alias when the post expires.
5. **Later:** internal DMs as an option, photos, specific user flows, stack and data model.

Session paused here. Resume tomorrow with todos 1 to 3.
