#!/usr/bin/env node
// Posts a single release-announcement tweet for @leanctx.
//
// Triggered from .github/workflows/release.yml after a stable release is
// created. Dependency-free (Node built-ins only) so the workflow needs no
// `npm install`. Auth is Twitter/X API v2 + OAuth 1.0a user context.
//
// Required env:
//   TWITTER_CONSUMER_KEY, TWITTER_CONSUMER_SECRET,
//   TWITTER_ACCESS_TOKEN, TWITTER_ACCESS_SECRET
//   RELEASE_TAG  e.g. "v3.6.26"
//   REPO         e.g. "yvgude/lean-ctx"
// Optional env:
//   DRY_RUN=1    compose + print the tweet, do not post
//   CHANGELOG    path to changelog (default: CHANGELOG.md)

import crypto from "node:crypto";
import https from "node:https";
import { readFileSync } from "node:fs";

const MAX_TWEET = 280;

function requireEnv(name) {
  const v = process.env[name];
  if (!v) {
    console.error(`Missing required env: ${name}`);
    process.exit(1);
  }
  return v;
}

/**
 * Pull a concise, human highlight for `version` from the changelog.
 * Prefers the section's blockquote summary (the "EPIC …" one-liner); falls
 * back to a factual count of Added/Fixed/Changed/Security entries.
 * Returns "" when nothing meaningful is available (caller posts version+link).
 */
function extractHighlight(changelog, version) {
  const lines = changelog.split(/\r?\n/);
  // Match `## [<version>]` literally — `version` is data, never compiled into a
  // RegExp, so there is no escaping to get wrong and no regex-injection surface.
  const needle = `[${version}]`;
  const start = lines.findIndex(
    (l) => l.startsWith("##") && l.slice(2).trimStart().startsWith(needle),
  );
  if (start === -1) return "";

  const section = [];
  for (let i = start + 1; i < lines.length; i++) {
    if (/^##\s*\[/.test(lines[i])) break;
    section.push(lines[i]);
  }

  // Join the section's leading blockquote (the "EPIC …" summary) into one line.
  const quoteLines = [];
  for (const l of section) {
    if (/^\s*>/.test(l)) {
      quoteLines.push(l.replace(/^\s*>\s?/, ""));
    } else if (quoteLines.length) {
      break; // end of the contiguous blockquote block
    } else if (l.trim() === "") {
      continue; // skip blank lines before the quote
    } else {
      break; // section starts with real content -> no summary quote
    }
  }
  if (quoteLines.length) {
    const clean = quoteLines.join(" ").replace(/\*\*/g, "").replace(/\s+/g, " ").trim();
    if (clean) return clean;
  }

  const counts = {};
  let cat = null;
  for (const l of section) {
    const h = l.match(/^###\s+(\w+)/);
    if (h) {
      cat = h[1];
      continue;
    }
    if (cat && /^\s*-\s+/.test(l)) counts[cat] = (counts[cat] || 0) + 1;
  }
  const order = ["Added", "Fixed", "Changed", "Security"];
  const labels = {
    Added: ["new feature", "new features"],
    Fixed: ["fix", "fixes"],
    Changed: ["change", "changes"],
    Security: ["security fix", "security fixes"],
  };
  const parts = order
    .filter((c) => counts[c])
    .map((c) => `${counts[c]} ${labels[c][counts[c] === 1 ? 0 : 1]}`);
  return parts.length ? parts.join(" · ") : "";
}

function composeTweet(tag, repo, highlight) {
  const url = `https://github.com/${repo}/releases/tag/${tag}`;
  const header = `🚀 lean-ctx ${tag} is out`;
  // Twitter counts every URL as 23 chars (t.co), so reserve that, not url.length.
  const urlCost = 23;
  let body = header;
  if (highlight) {
    const budget = MAX_TWEET - header.length - urlCost - 4; // 2×"\n\n"
    let h = highlight;
    if (h.length > budget) h = h.slice(0, Math.max(0, budget - 1)).trimEnd() + "…";
    if (h) body += `\n\n${h}`;
  }
  return `${body}\n\n${url}`;
}

function postTweet(text) {
  const CK = requireEnv("TWITTER_CONSUMER_KEY");
  const CS = requireEnv("TWITTER_CONSUMER_SECRET");
  const AT = requireEnv("TWITTER_ACCESS_TOKEN");
  const AS = requireEnv("TWITTER_ACCESS_SECRET");

  const endpoint = "https://api.twitter.com/2/tweets";
  const pct = (s) =>
    encodeURIComponent(s).replace(/[!'()*]/g, (c) => "%" + c.charCodeAt(0).toString(16).toUpperCase());

  const oauth = {
    oauth_consumer_key: CK,
    oauth_nonce: crypto.randomBytes(16).toString("hex"),
    oauth_signature_method: "HMAC-SHA1",
    oauth_timestamp: Math.floor(Date.now() / 1000).toString(),
    oauth_token: AT,
    oauth_version: "1.0",
  };
  const paramStr = Object.keys(oauth)
    .sort()
    .map((k) => `${pct(k)}=${pct(oauth[k])}`)
    .join("&");
  const baseStr = `POST&${pct(endpoint)}&${pct(paramStr)}`;
  const signingKey = `${pct(CS)}&${pct(AS)}`;
  const signature = crypto.createHmac("sha1", signingKey).update(baseStr).digest("base64");
  const authHeader =
    "OAuth " +
    Object.entries({ ...oauth, oauth_signature: signature })
      .map(([k, v]) => `${k}="${pct(v)}"`)
      .join(", ");

  const payload = JSON.stringify({ text });

  return new Promise((resolve, reject) => {
    const req = https.request(
      endpoint,
      {
        method: "POST",
        headers: {
          Authorization: authHeader,
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(payload),
        },
      },
      (res) => {
        let data = "";
        res.on("data", (c) => (data += c));
        res.on("end", () => resolve({ status: res.statusCode, body: data }));
      },
    );
    req.on("error", reject);
    req.write(payload);
    req.end();
  });
}

async function main() {
  const tag = requireEnv("RELEASE_TAG");
  const repo = requireEnv("REPO");
  const version = tag.replace(/^v/, "");
  const changelogPath = process.env.CHANGELOG || "CHANGELOG.md";

  let highlight = "";
  try {
    highlight = extractHighlight(readFileSync(changelogPath, "utf8"), version);
  } catch (e) {
    console.warn(`Could not read ${changelogPath}: ${e.message} — posting version + link only`);
  }

  const tweet = composeTweet(tag, repo, highlight);
  // Twitter weights every URL as 23 chars (t.co), regardless of real length.
  const weighted = tweet.replace(/https?:\/\/\S+/g, "x".repeat(23)).length;
  console.log(`Tweet (${weighted}/${MAX_TWEET} weighted chars):\n---\n${tweet}\n---`);

  if (process.env.DRY_RUN === "1" || process.argv.includes("--dry-run")) {
    console.log("DRY_RUN — not posting.");
    return;
  }

  const res = await postTweet(tweet);
  if (res.status === 403 && res.body.includes("duplicate content")) {
    console.log("Tweet already posted (duplicate) — treating as success.");
    return;
  }
  if (res.status !== 201) {
    console.error(`Twitter API error ${res.status}: ${res.body}`);
    process.exit(1);
  }
  let id = "";
  try {
    id = JSON.parse(res.body).data.id;
  } catch {
    /* keep id empty */
  }
  console.log(`Posted: https://x.com/leanctx/status/${id}`);
}

main().catch((e) => {
  console.error("Fatal:", e);
  process.exit(1);
});

