# Plan — Transcript actions and export

**Status:** In progress. Phase 0 has begun: transcript row indices now map to user-turn ordinals, ambiguous duplicate prompts require an explicit selection, and a successful fork parks the source session's draft and attachments in the existing in-memory draft store. If the source path is unknown, its draft remains in the composer rather than being overwritten. Cancellation leaves the draft untouched. The fork-point picker ignores replies from dismissed or superseded requests. Session switches, new tasks, cloning, folder changes, prompt submission, and explicit process stop/restart or app reload are guarded while a fork reply is pending; the focused rewind tests pass. Other process lifecycle paths still need review. Remaining: test the actual pointer path and verify with real pi. P1 has begun with per-response Markdown export and palette-driven visible-transcript export; P2–P4 are not implemented. Preserve all existing work, including the intentional deletion of `docs/plans/review-guide.md`.

## Goal and scope

Make actions on messages and conversations discoverable and trustworthy: fork before a prompt, reuse a prompt, and export a response or a conversation. Orbit remains a native GPUI application; pi remains the session source of truth. Export must never send conversation data to a remote service.

## Phases

1. **P0 — Reliable fork (start here).** Correct the mapping between transcript rows and pi's fork points. Never silently preselect a different prompt when identity is ambiguous. Preserve the original session, draft, images, and filesystem; handle cancellation, stale responses, running sessions, and reconnection honestly. Test repeated prompts, compaction, abandoned branches, intervening assistant/summary rows, and actual pointer-to-picker behavior. Manually verify with real pi. Expose clearer fork/edit-and-resend wording only when behavior is proven.
2. **P1 — Markdown export (in progress).** The response footer offers per-response export, and the command palette offers visible-transcript export. The native asynchronous save dialog snapshots text before opening; background writes refuse overwrites. Thinking and tool output are omitted, and image attachments are noted rather than embedded. Tests cover Markdown text, omitted content, empty transcripts, and no-overwrite. Remaining: manual GUI verification, clear pre-export disclosure of the visible-projection/compaction limitation, optional thinking/tool output, large-session and error/cancellation coverage.
3. **P2 — PDF export.** Investigate pi's export commands and native/offline Rust PDF options against Orbit's no-webview and cross-platform constraints before choosing an implementation. Export response or conversation via a native save dialog, with readable code, tables, images and page breaks. Provide a preview if technically feasible and disclose unsupported content. Test deterministic rendering, Unicode, pagination, long code and cancellation; manually inspect macOS, Windows and Linux output.
4. **P3 — Historical actions.** Evaluate regenerate/retry and edit-and-resend against the installed pi RPC contract. Never imply a destructive in-place edit when pi creates a fork. Preserve unsent drafts and attachments, and require explicit user action before replacing either. Add tests for failures, cancellation, busy sessions, and session changes.
5. **P4 — Navigation and sharing (separate decisions).** Design persistent bookmarks/outline after stable entry identities are available. Compare forks only after defining a comparison model. A hosted sharing service is out of scope unless separately approved; local export does not upload data.

## Verification

For each phase: `cargo check -p orbit-pi`; focused `cargo test -p orbit-pi --bin orbit-pi <filter>`; relevant keyboard, states, motion and i18n scripts for UI/string changes; and a manual GUI pass. Update `README.md`, `INTENT.md` and `CHANGELOG.md` only when a feature is verified. State platform and testing limitations explicitly.

## Open decisions

- PDF library/backend and exact fallback behavior for unsupported content.
- What a preview should include, how attachments are represented, and whether user-selected destination files may be overwritten.
- Whether regenerate creates a new session or branches within the same one; pi's actual contract governs.
