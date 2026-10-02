# Orbit Privacy

Orbit is local-first. Your source code, projects, terminal, and AI
conversations are yours; analytics is a small, optional stream of product usage
information used to improve the app.

## Anonymous usage analytics

Orbit can send a limited set of anonymous usage information to a self-hosted
analytics server (`telemetry.rajeshwar.tech`). This is **on by default in
release builds and off in development builds**, and you can turn it off at any
time in **Settings → Privacy → Anonymous Usage Analytics**.

When enabled, Orbit may send:

* a **pseudonymous installation identifier** — a random UUID generated on first
  run, stored locally in `~/.orbit-pi/analytics.json`;
* a **session identifier** — a new random UUID for each launch;
* the app version, operating system, CPU architecture, and interface locale;
* a small set of feature-usage events, such as: the app started/closed, a
  project was added/opened/removed, the terminal was opened, an agent run
  started/finished/failed, an MCP server connected/disconnected/failed, the
  command palette or settings opened, the theme or a setting changed, and an
  update was available or installed.

Event details are limited to coarse values (for example, a failure *category*
such as `provider`, or a theme `mode` such as `dark`). No free-form text is
sent.

### The installation identifier is pseudonymous

The installation identifier is a random number. It is not derived from your
email, username, account, hostname, MAC address, CPU serial, disk serial, or any
hardware identifier. Because it is stable across launches it can be linked to
one installation over time, so it is best described as **pseudonymous** rather
than legally anonymous.

Resetting it (not exposed in the UI today) generates a new identifier and a new
session and discards anything queued.

## What Orbit does not intentionally collect

* source code, file names, file contents, or project/repository names or paths;
* terminal commands or terminal output;
* AI prompts or responses, or conversation contents;
* API keys, access tokens, passwords, MCP credentials, or auth cookies;
* environment variables;
* clipboard contents;
* IP address as an analytics property, exact location, or private URLs;
* machine hostname, MAC address, serial numbers, or disk identifiers.

## No blocking, no dark patterns

Telemetry never blocks the app, never shows an error when the network is down,
and never delays quitting for more than a short best-effort flush. Turning it
off stops collection immediately and clears anything already queued. Orbit works
exactly the same with analytics disabled.

Analytics data is sent over HTTPS only; TLS verification is never disabled.

## Source of truth

The implementation and every event are documented in
[`docs/analytics.md`](docs/analytics.md) and the `crates/orbit-analytics` crate.
