# Changelog

All notable changes to Orbit Pi are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- **Plugin notifications keep their level (#173).** A toast an extension raises
  with `ctx.ui.notify` now shows as a warning or an error when the extension
  said so — tinted, and on screen longer — instead of every one arriving as an
  info card. The In-app toasts switch in Settings → General still turns them
  all off.

- **Default thinking level on Providers & models.** The default-model card on
  Settings → Providers & models now has a **Thinking level** row under the
  model, offering that model's own levels. It edits the same new-session
  default as Settings → Agent, so the two pages never disagree; until a default
  model is picked the row stays a disabled *pi default*.

- **Subagents get their own dock, not Review's (#190).** A delegated agent
  opened from its card ("Open in Subagents") now lands in a **Subagents** panel
  of its own instead of a tab on the Review pane: one entry per subagent with
  its state, name and elapsed time, and the selected one's response below. Review
  shows the diff and nothing else. The two trade the right side at the same
  width, a top-bar chip brings Subagents back, closing an entry falls to its
  neighbour, and closing the last one hides the panel.

- **Quote a transcript passage into the composer (#187).** Select text in the
  transcript and choose **Quote in Composer** from its right-click menu (first
  item, keyboard-reachable like the rest). The passage becomes a quote above the
  input with an optional one-line comment; Enter keeps the comment, Escape undoes
  it, and Backspace in an empty comment removes the quote. Sending — or steering
  a live run — leads the prompt with each quote as a `>` block and its comment,
  then your text, and the sent message shows the quotes above the bubble, live
  and after a reload. Quotes carry the rendered text, so `**bold**` quotes as
  `bold`. Up to eight per message, 4,000 characters each; they survive switching
  sessions and a failed send.

- **A toolbar for transcript selections.** Selecting text (a drag, or a double or
  triple click) shows the text menu beside the selection — Quote in Composer,
  Copy Selection, Copy Message — without taking the keyboard from the composer.
  Escape or a scroll closes it and keeps the selection, and the next press starts
  a new selection straight away. **Shift+F10** opens the same menu at the
  selection's end from the keyboard (the sidebar keeps Shift+F10 for its row menu).

- **A settings page opens at its top, and its pinned header stays above it.**
  Two layout faults made the Providers & models page look cut off: a section kept the
  previous page's scroll offset (so it opened scrolled down, its first card shaved off
  under the toolbar), and the header's wrapping description was measured as one line
  while it painted as several — the header then came out short and its toolbar painted
  over the first card. The description is now one line with the full sentence on hover,
  the scroll offset resets per section, and both faults have layout tests.

- **Cleaner Providers & models and plugin cards.** The Models page's count line under
  the toolbar ("N providers · N active · …") is gone, and a plugin card ends at its
  badges instead of repeating the package's install path.

- **Plugin page round-out.** A configured-but-missing package can be installed
  from its card again, disabled packages sort below the active ones with a line
  saying what "disabled" means, plugin results report in the page's own status
  line instead of a corner toast, and **Restart pi** sits beside Refresh so a
  package change loads without leaving the page.

- **New projects are named where they are created.** Creating a project opens
  the rename dialog seeded with the folder's name, so it can be given a display
  name immediately instead of hunting for the header menu.

- **One Orbit instance on Windows.** A second launch (a desktop shortcut, a
  double-clicked exe) raises the running window and exits instead of starting a
  second app and a second agent.

- **Per-plugin enable/disable toggle.** Each installed plugin on the Settings →
  Plugins page now carries a switch that flips pi's own `autoload: false` form
  in `settings.json`. A disabled package stops loading on pi's next start or
  `/reload`, and the switch reflects the on-disk state — never a decorative
  toggle.

- **Sidebar session rename opens immediately.** Clicking Rename no longer
  opens the conversation first. Only saving a changed name loads the target
  session; Cancel and unchanged names leave the current conversation in place.

- **Generate a title from Rename Session.** The dialog now offers the same
  wand as Session Details. It applies the generated title immediately and
  keeps any edits typed while generation was in flight.

- **Consistent rename dialogs.** Sessions and projects now share a compact form
  with a full-width name field, close control, and a dedicated Cancel / Save
  changes footer. Renaming a session no longer opens or rearranges the details
  dock. Project renames still change only the sidebar display name.

- **Every confirmation asks a question and says the consequence.** The shared card
  (`ui/confirm.rs`) was fed unevenly by `app/confirm_ui.rs`: most prompts had no line
  saying what happens, four carried no mark beside the question, and three put the
  consequence inside the title. Each prompt now has a question-only title, one
  consequence line (what is deleted or removed, where from, and whether it can be
  undone), the affected object in the target block, and the trash glyph on the
  shared danger-washed icon tile (`TileLook::Destructive`) above the question.
  Clean up now
  (session retention) no longer uses the button's label as its title, and
  removing a remote host, an MCP server, a provider, a plugin or an account now
  says explicitly what is removed and what is left alone. Skills and subagents
  name themselves above their file path instead of burying the path in the
  question. A layout test pins that a prompt with a longer header still keeps its
  footer in view at the largest UI size.

- **Centered destructive confirmations.** The icon, question and consequence
  stack on the center axis, with no divider above the target details. A single
  divider separates the inset footer, which pairs equal-width outlined Cancel
  and soft danger buttons. Simple prompts are narrower; session and project
  deletion still have room for long paths.

- **One confirmation prompt for every destructive action.** Deleting a session, a
  project folder, a skill, a subagent, a provider, a plugin, an account, an MCP
  server or a remote host, removing a worktree, deleting a file from Explorer,
  discarding changes in Git, and Clean up now (session retention) all ask through
  the same blocking card (`ui/confirm.rs` + `app/confirm_ui.rs`). The question,
  what happens, and the object affected are on the card; Cancel takes the
  keyboard when it opens, Escape cancels, Tab reaches the destructive button, and
  focus returns to the composer on close. Replaces seven inline row/menu confirm
  states and three hand-rolled cards, most of which had no keyboard route. Also
  fixed: a `Danger`/`Destructive`/`Tinted`/`Glass` button panicked the frame under
  Reduce Motion. Delete Session now shows the full title and the session file
  and workspace paths (when known). Long details scroll without hiding the
  actions, and Enter respects the focused button instead of always confirming.

- **Default model in Providers & models.** The model selector now edits the same persisted new-session default as Settings → Agent instead of changing only the current session. The composer still controls the active session's model.

- **Local sessions follow `PI_CODING_AGENT_DIR`.** The sidebar index, its watcher, the
usage scan and the onboarding path row all located the store at `~/.pi/agent/sessions`, so a
pi configured to keep its data elsewhere kept writing sessions Orbit could not see. They now
resolve through pi's own agent directory — `PI_CODING_AGENT_DIR`, or `~/.pi/agent` — including
a leading `~` in the value, which pi expands internally. `PI_CODING_AGENT_SESSION_DIR` takes
precedence (below); `settings.json › sessionDir` still moves the store unseen.

- **Swap-hover buttons render under Reduce Motion.** Tinted, Danger, Destructive and Glass
buttons carry their one-frame hover swap from `variant_treatment`; under Reduce Motion the
zero-duration branch of `motion::fade` set a second hover style on the same element, and
gpui's debug assertion (`hover style already set`) ended the process — Settings → Subagents
was the reported crash site. The branch now applies its hover closure only when the `Hover`
carries a fill or ink, so a variant's own swap stays the single hover style and every page
using these variants renders under Reduce Motion.

- **Local sessions follow `PI_CODING_AGENT_SESSION_DIR`.** pi resolves its session store as
`--session-dir` > `PI_CODING_AGENT_SESSION_DIR` > `settings.json › sessionDir` >
`$agentDir/sessions`; Orbit passes no `--session-dir`, so a user who set the environment
variable still got an empty sidebar after the agent-directory fix. The session index, its
watcher, the retention scan, Usage and the Settings path row now prefer
`PI_CODING_AGENT_SESSION_DIR` (a leading `~` expanded, empty values ignored) and fall back to
the agent directory's `sessions/`. `settings.json › sessionDir` remains unwired.

- **Selecting back rightward shrinks a leftward word selection.** After Ctrl/Alt+Shift+Left
selected words leftward, Ctrl/Alt+Shift+Right started from the selection's right end and the
whole leftward selection was dropped. `select_word_right` now starts from the caret end
(`cursor_offset()`), so selecting back shrinks the selection toward the anchor the way a
native text field does.
- **Ctrl/Alt+Backspace deletes a word.** Ctrl+Backspace (Linux/Windows) and ⌥Backspace (macOS)
in the composer and the Explorer editor were unbound, so the keystroke did nothing. A new
`DeleteWordBackward`
action selects to the previous word boundary — the same boundary Ctrl/Alt+Left moves to — and
deletes through the existing backspace path, so the IME composition guard, the undo stack and
the empty-selection no-op all hold; the two chords point at one action in both text contexts.

- **Opening a large folder no longer freezes the window.** The Explorer built its
whole tree — one insert per entry, then a sort per directory — *after* the
background walk returned, on the UI thread. A folder holding many files in one
directory made that quadratic, because every insert scanned the siblings already
there: 20 000 files in a single folder cost ~1.5 s of the main thread, which is the
beachball in the report. The build now runs in the same background task as the
walk and the UI thread only stores the finished index. The insert itself is
linear — the snapshot is sorted by path component before it is walked, so a
sibling that already exists is always the last child — which is ~60× faster on a
wide directory and leaves wide workspaces unaffected. The walk also bumps a
generation on a workspace switch or the hidden-files toggle, and a superseded
walk now hands the `loading` slot to the request it superseded instead of
dropping it (the panel could previously sit on a spinner until another event
arrived), and `render` no longer re-flattens the whole tree on every frame.

- **Ctrl+Left/Right moves by word on Linux and Windows.** `secondary-left/right` names the
platform's primary modifier, which off macOS is `ctrl`, so those chords jumped to the ends of
the line and the standard word movement — already implemented and bound to `alt`-arrow — was
unreachable. Ctrl+Left/Right and Ctrl+Shift+Left/Right now resolve to `WordLeft`/`WordRight`
and the `Select*` pair in the composer and the Files editor; the platform block is registered
after the line bindings so it wins by keymap precedence. Home/End still reach the line edges,
and macOS (⌘← for the line, ⌥← for a word) is unchanged. Covered by
`ctrl_arrow_moves_by_word_off_macos`.

- **Release packaging no longer sinks a build on one flaky `hdiutil` call.** Hosted macOS
runners intermittently return `hdiutil: create failed - Resource busy` while building a DMG
(`actions/runner-images#7522`) — not a bad image, a momentarily busy APFS container. It hit
the 0.2.21 release run on the universal DMG, which aborted the macOS job *after* the arm64
DMG had been built, signed, notarized and stapled, so no macOS artifact was uploaded at all.
`make-dmg.sh` now retries the create with a backoff (clearing a leftover mount of the
previous pass first, and force-detaching when a plain detach sticks), and gives each pass its
own staging directory and writable image so arm64 and universal can no longer wedge each
other. The draft-release job also refuses to cut the version tag when the macOS job failed:
a release without its DMGs and updater archives is not shippable, and a published release is
never rebuilt.

## [0.2.21] - 2026-11-24

- **Decision cards lay their options out on a line again.** The inline "Decision needed"
card (an extension's `select`, such as *Finish review*) and the questionnaire panel share
`ChoiceRow`, whose label sat in a wrapping flex row with an auto basis and `min_w_0`. That
let the label shrink to zero width, so every option painted one glyph per line ("R / e / t /
u / r / n…") down a tall empty row. The label now takes the line's remaining width and wraps
inside it, and the Recommended badge keeps its own width beside the first line. Covered by a
layout test of the card's own container chain.

- **The changed-files summary reads as a card again.** The end-of-task "Changed N files"
summary with its Review button was the only transcript card on its own fill: a
`surface_secondary` well at the wallpaper's card opacity, with no edge. In dark palettes
that landed a shade *below* the canvas (measured 1.02:1, under the design system's 1.045:1
visible step), so the card sank into the background. It now sits on the same inspector
material as the tool, thought and delegation cards beside it (`frosted_transcript_fill`):
contrast-floored over glass, the page-glass ink lift over a full-window wallpaper, and the
`overlay` plate where it is opaque.

- **Markdown images show inline.** An image in a GitHub issue or pull request body, or
in an agent's reply, used to render as an `image · Open` card that only handed the URL to
the browser. It now loads inline through GPUI's image cache and the same bounded HTTPS
client avatars use (5 s connect / 15 s total timeouts, 5 redirects, an 8 MiB cap), and
lays out at its own width — narrowed to the column and to a 30 rem height — with its
aspect ratio kept; clicking it opens it in the browser. A skeleton bar holds its place
while it loads, and an image that cannot load (a private attachment, a dead link, a
plain `http://` URL) falls back to the alt text and **Open**. `file://` and `data:`
images still never load, and a host that resolves to a private, loopback or
link-local address is refused. This reverses the earlier rule that remote
Markdown images are never fetched: `PRIVACY.md` now says what an image host
can see.

- **Keycaps centre their marks.** The `↵ Queue` / `⌥ ↵ Steer` hints, the sidebar's `⌘1`…
slots, the palette hint and the working-folder key legend drew `↵`, `⌥`, `⌘`, `⌃` and `⇧`
as text, each from whichever font covered it and at that font's own height; text layout
centres the line box rather than the ink, so `↵` rode about 1.5px above the middle of its
cap and above the `⌥` beside it. `kbd_key` now draws those marks with the bundled Symbols
Nerd Font's keyboard icons, which sit on the middle of the em box. Words and letters
(`Esc`, `Tab`, `Enter`, `B`) stay text, and the caps keep their size.

- **The Subagents save button only lights up when there is something to save.** The page's
one primary button read “Create override” whether or not the form had been touched, so
editing a built-in (`reviewer`, `scout`, `worker`) showed an unsaved marker and no save,
while a reopened agent offered a Save that wrote the same bytes back; the sentence
explaining that saving a built-in writes an override sat in the File section below the
fold. The button is now **Save** for anything with a file or a built-in behind it and
**Create** for a draft, disabled until the form differs from what was loaded, with the
override path in its tooltip while it is still disabled. A built-in renamed before its
first save still says Create, because that writes a new agent rather than an override.

- **Leaving Subagents with unsaved edits says where the click was going.** Clicking
another section in the settings nav while an agent form had unsaved changes held the
navigation and asked “Discard unsaved changes to scout?”, which never named the
section, so the click looked dropped and the pane looked stuck; the nav cursor also
moved to the refused row, leaving the keyboard out of step with the page on screen.
The question now ends in *and open Providers & models?*, and the cursor stays on the
section that is actually open until the edits are discarded.

- **Two-line rows hold their caption closer to the title.** Every picker row with a
secondary line (the model and thinking pickers, Work in, the folder picker, worktrees,
remote hosts, the access and workflow menus, Usage filters and the palette's empty
state) stacked its title and caption on the `Comfortable` 1.618× leading, which put
about 8px of air between the two lines; the access and workflow menus added 2px more.
They now stack on one token, `picker::STACK_LEADING` (`Standard`, 1.3×), with no extra
margin, so a two-line row is 42px instead of 50px at the default size. One-line rows,
trailing title slots and leading marks stay aligned to the title's line, and the
row-height token they are all measured from changed with them.

- **The model and thinking pickers open wholly above their chip.** In a short window the
model picker's six-row list was taller than the room above the composer, and gpui's
`anchored` answers a menu that does not fit by snapping it back into the window, so the
picker slid down over its own chip and the click meant to close it landed on the picker.
The composer now records the chip row's top edge as it lays out, and the picker caps its
height to the room above it (less the menu gap and the window margin). Only the list gives
way and scrolls; the search, tabs and footer keep their size, and keyboard navigation
scrolls against the list's painted height. A cramped window still keeps a few rows.

- **Tabs and sidebar row actions fade too.** Every tab — the terminal's, the right dock's
and the Git page's — swapped its hover on one frame, because `ui/tab.rs` painted it with a
plain `.hover` style. The tab now fades through `motion::fade`, with the same wash and the
same muted-to-secondary ink, and callers still chain `on_click`, tooltips and debug
selectors after `render`. A sidebar project header's `…` and `+` actions were revealed by
`group_hover`, so they popped in while the header's wash faded. They now follow the
header's own hover transition (`motion::follow`): no second clock, so the actions and the
wash cannot drift a frame apart. An open header menu still holds its `…` lit, and Reduce
Motion falls back to the one-frame reveal.

- **Hovers fade across the component layer.** `motion::fade` wraps a control and still accepts
everything a chain does after it — styling, handlers, children — because gpui's `Styled`,
`InteractiveElement` and `StatefulInteractiveElement` each have one required hook and several
hundred default methods built on it. So the components could adopt it without changing a
single call site: every `Button` and `IconButton` (about 260 call sites), every Explorer and
review tree row, the flat top-bar controls, and the transcript's three block toggles now
cross-fade their fill and ink instead of swapping on one frame. Variants whose hover changes
more than a fill (Tinted, Danger, Destructive, Glass) keep the one-frame swap, because their
hover is not a fill.

- **A hover fade can take children now.** A fade has to wrap the element it fades, which made
it the last call in a chain: every call site had to move its `.child(…)` calls above it, and
that is why most hovering controls in the app still swap on one frame. `motion::fade` collects
the children instead, so a site converts by wrapping its chain — the transcript's code-block
fold, its diagram-source toggle and its changed-files toggle are the first. `motion::hover` is
the same implementation as a leaf, so its existing adopters are unchanged.

- **The session preview can be read without a mouse.** The card is the only place the
sidebar shows a session's full path and provider list, and it can be taller than the
window — but its overflow was pointer-only, so a keyboard reader could never reach the
end of it. Page Up / Page Down now page the card while it is showing (the pointer's, or
the one the keyboard cursor paints), stopping at both ends. It scrolls the same handle
the wheel does, and a dismissed card opens at the top again.

- **The About page's header no longer pushes its board off the screen.** The identity
block put its text column in a row without letting it grow, so the column was sized to
its max-content — for a nested flex column, the 98px title. The subtitle wrapped to five
lines and the version line to three, the header grew to roughly 1,500px, and the board
(updates, runtime, source) began about 1,600px down a 640px-tall window's scroll area;
the mark also had no definite box, so its width came from the decoded asset and the row
reflowed when it arrived. The column now takes the row's remaining width, and the mark is
a 48px box. Every other Settings page was unaffected, because there the same column sits
in a column parent, where a flex item stretches.

- **Panels slide instead of snapping.** The sessions sidebar, Review (and a subagent
tab), the Explorer, session details and the bottom terminal strip now ease their own
extent when they open or close — a width for the docks, a height for the terminal — and
the space beside or above them is given back on the same curve. The sidebar's slide was
hand-rolled on a counter bumped by the toggle, which could not be interrupted: closing
and reopening quickly restarted it from the end. All of them now share one primitive
(`motion::panel::slide`) built on the motion layer's retargetable `Transition`, so a
panel caught mid-slide resumes from the extent on screen. A closing panel stays mounted
for the slide and leaves the tree once it has arrived, so a closed dock still costs
nothing; the sessions sidebar stays mounted while closed because its focus handle has to
be in the tree for ⌘⇧B. The terminal was the case that had never had a chance to
animate: its strip was gated on "visible", which folded in its open flag, so closing it
unmounted the row on the same frame. The top bar's leading pad and the titlebar controls
ride the same clock as the sidebar's edge rather than animating their own. Reduce Motion
takes the end state on the frame it changes. The Git page still snaps: it replaces the
whole column rather than sitting beside it, so it needs a page entrance, not a width.

- **Pages and Settings sections arrive instead of appearing.** Opening Git, Usage, Files,
Worktrees, an expanded terminal or any Settings section now fades its content in
(`motion::arrive`, on the layout role) instead of replacing the previous one on a single
frame. Only the content fades, over the card's own canvas — the card, its gutter and the
Settings nav never move — so nothing flashes the wallpaper through the swap, and the page
never travels: an offset would reflow the page under the reader's cursor for a picture
that was already gone. The destination, not the moment, names the entrance, so swapping
between two pages never inherits the other's finished fade, and typing in Settings →
Search does not re-fade on every keystroke. A right-dock tab switch stays instant: it
swaps between transcripts, and the transcript takes no entry animation. Reduce Motion
paints the page opaque on the frame it opens.

- **Spelling marks in the prompt composer.** Misspelled words in the composer
  get a wavy underline once you pause typing, drawn the way a native text view
  draws them. Orbit paints its own text, so macOS never spell-checked it; the
  new `spellcheck` module asks `NSSpellChecker` for the misspelled words and
  marks only prose — fenced and inline code, paths, identifiers, commands and
  `@`-mentions are skipped, and the run lengths are untouched so the caret,
  selection and wrapping never move. There is no auto-correction and no grammar
  checking (a prompt is imperatives and code, which Apple's grammar checker
  flags even when it is right). Right-click a marked word for the checker's
  suggestions (up to five, best first) and **Learn spelling** to add it to the
  macOS dictionary; the menu opens only on a marked word, so the squiggle and
  the menu never disagree. Settings → Agent → Check spelling turns it off.
  macOS only — Windows and Linux have no checker in this stack.

- **Clearer session previews.** Sidebar hover and keyboard preview cards group
  the title, location and providers, give branch/divergence their own row, show
  paths in monospace, align metadata icons/text, and separate the shortcut hint into
  a quiet footer. Viewport-bounded scrolling keeps full paths, branches and all
  provider/model labels readable without truncation; a short pointer handoff grace
  allows entering the popup to scroll. The keyboard preview route is retained.

- **Search within a provider's models.** Each expanded Models section has its
  own search field for model ids and names. It keeps your query when collapsed;
  Show all / Hide all still apply to the whole section, not just search results.

- **Git activity at a glance.** The Git button at the bottom of the sidebar
  shows a dot when you have changed files, or when new issues or pull requests
  were opened on GitHub since you last looked; hover it to see which. On the
  Git page, the Changes tab shows how many files changed, and Issues and Pull
  requests show how many are new until you open them. The GitHub check uses
  your signed-in `gh`, runs every 5 minutes and when the window comes forward,
  ignores items you opened yourself, and can be turned off in Settings →
  Privacy → GitHub.
- **Maximized window reopens maximized.** A window you closed while
  maximized could reopen a little short of the screen and no longer
  maximized. Orbit now waits for the maximize animation to finish before
  remembering the window's size, so the size to restore to is the one you
  had before maximizing.
- **Tighter workbench cards with clean corners.** The Files, Git, Usage and
  Worktrees pages and the Explorer, Review and Session details docks now sit
  6px apart (down from 12px), and keep the same 6px from the window's right
  and bottom edges instead of running into the window's own rounded corner.
  A file or page no longer paints its last lines through the card's bottom
  corners and edge, and every card's outline stays visible at all four
  corners.
- **Even file tab header.** An open file's tab now sits centred in its strip
  at every UI size, the same distance from the top and left of the card, and
  the close button matches the tab's height. The file name, the path below it
  and the size/lines/Saved details now line up on one left and one right edge.
- **New Session details icon.** The top-bar button that opens Session details
  now shows a list icon instead of the info "i", which stays in use for
  notices and About.
- **Centred Jump to latest arrow.** The arrow in the round "Jump to latest"
  button now sits exactly in the middle of its circle; it used to be pulled
  1px up and to the left.
- **Cleaner Explorer panel.** The project panel now keeps a minimum width of
  260px (up from 220px) and opens at 280px, so the workspace name and nested
  file names stay readable. Long file and folder names end in "…" instead of
  being cut off, and hovering a row shows its full path. The filter box now
  lines up with the file tree. When a filter matches nothing, the panel says
  so instead of claiming the workspace has no files.
- **No focus rings.** Buttons, chips, rows, fields, switches, Settings, the
  Explorer tree and transcript disclosures no longer draw a highlighted ring
  when they have keyboard focus. Tab, the arrow keys, Enter and Space still
  work as before; nothing moves or changes size.
- **Project icons without a box.** Sidebar project icons and logos now show
  on their own, with no tile or outline behind them. Icons keep the same size,
  so project names still line up.
- **Animated wallpaper effects.** Every wallpaper effect now moves like the
  new-session background: the same drifting pattern, Orbit monogram and
  sweeping light play over your picture in the effect's own style. Dither
  grain flows, halftone dots swell and shrink, ASCII characters change, and
  scanlines thicken and thin in place (they no longer roll, which made the
  screen flicker). The effect's pixel size applies, and Reduce
  Motion keeps it still.
- **Lighter background animation.** The new-session background and animated
  wallpaper now update 10 times a second instead of 30, which looks the same
  because the pattern drifts slowly. They pause while Orbit is in the
  background, and a wallpaper stays still behind chats, Settings and other
  pages, so the CPU and GPU do far less work.
- **Transcript cards are frosted glass again over a full-window background.**
  Tool, thought and subagent cards, a tool call's Arguments and Output area,
  code blocks and the composer now use the same frosted material as the
  sidebar. Before,
  each painted another near-opaque tint over the already-tinted chat column,
  so the frost behind it was almost hidden and an open tool card looked like a
  dark slab next to a subagent card. Code areas still get just enough extra
  tint to keep their text readable.
- **Buttons, chips and fields are frosted glass too.** Over a background (and
  in dark menus and dialogs), secondary, outline and danger buttons, outlined
  and file chips, segmented controls, icon tiles and text fields (including
  the Git page's Sync button, the composer's stop button and the subagent
  limit stepper, and every text field built on the shared field frame such as
  Settings → Plugins' install field) now show the
  frost through a light glass face instead of a solid dark one. Hover, open and
  selected states are lighter glass steps. Primary, accent and red delete
  buttons stay solid so they still stand out.
- **New switch style.** Every settings switch now uses a rounded-rectangle
  track with a rounded-square knob that slides across; it turns green when
  on. The design is adapted from RaspberryBee's Uiverse.io switch (MIT).
- **New loading spinner.** Loading indicators, including the spinner on a
  running task in the sidebar, are now one style: a faint full ring with a
  short arc turning on top.
- **Sidebar projects, easier to read and manage** (#166). The Projects label,
  project names and project icons are larger. A project's ⋯ menu has
  **Rename…**: the new name shows in the sidebar only, and the folder on disk
  is never renamed (leave the field empty to go back to the folder name).
  **New session** and **New project** now have their own row under search:
  a small **New project** button on the left and a wide **New session** button
  on the right. The search row keeps only search and the view menu.
- **Delete the open task** (#166). **Delete session…** now works on the task
  you have open: Orbit opens a new task, stops the old task's pi process, then
  deletes the file. It stays disabled only while a run is in progress. The
  command palette's Delete Session uses the same safe path.
- **Branch from a message…** (#166) in a task's menu opens the message picker
  for that task, so you can start a new task from any earlier turn.
- **Work in picker, reorganised** (#166). The new-task page's **Work in**
  picker now has two labelled sections instead of hairlines: **This device**
  (current checkout, New worktree, Existing worktrees) and **Remote** (your
  SSH hosts, Add this project to a server…, and a new **Add remote host…**
  row). Remote is always there, even before you set up a host: Add remote
  host… opens Settings → Remote hosts with the form ready. The Experimental tag
  now sits once on the Remote label instead of on every host.
- **Cleaner command cards.** An open command card shows its command once:
  the header no longer repeats a cut-off copy, and the command block drops
  its "Command" label (the `$` already says it). Output keeps one line per
  row and scrolls sideways instead of wrapping into a ragged block, and
  `file:line:` locations from grep, rg or a compiler are dimmed so the
  matched text stands out.
- **Inline code looks like a chip** (#168). Code in answers and thinking
  (`origin/main`, `npm run build`) now has rounded 4px corners, a little space
  on each side, and a background fitted to the text instead of a hard
  rectangle as tall as the line. It keeps the same colour in every theme.
  Selected text still highlights as a plain block.
- **Stop matches Send.** The composer's Stop button now shows a small filled
  square instead of a full circle with a square cut out. The old mark filled its
  whole icon box and looked like a bigger, heavier circle inside the round
  button, so Stop read larger than Send even though both are 28px.
- **Corner rounding setting removed.** Settings → Appearance no longer has a
  Corner rounding control; Orbit's corners are fixed at their standard sizes.
  A value saved by an earlier version is ignored.
- **First launch no longer uses Orbit's own folder as the project** (#166). A
  launch from the Dock, Start menu or a shortcut with no remembered folder now
  starts in your home folder instead of the install directory (Windows) or `/`.

- **Frosted code blocks.** Code blocks and Mermaid diagrams in answers now
  use the same frosted glass as tool cards over a wallpaper (and in dark
  themes), with the code well's contrast-floored tint so code stays readable.
  Light themes, Reduce Transparency and Increase Contrast keep the solid card.

- **Watch a subagent work, live.** Opening a delegated agent in the side pane
  now shows its own transcript as it streams — the files it reads, the
  commands it runs with their output, its thinking and its answer — drawn
  exactly like the main chat, and a finished agent replays the same way. A
  status line above it says what state it is in and for how long. Each tab
  shows the agent's state (running, done, failed), and two agents with the
  same name are numbered (`explore · 1`, `explore · 2`). The log behind this
  stays on your machine and is cleared after a week.

- **Quieter transcript activity.** Inside an expanded activity group, thoughts
  and finished tool calls are now flat rows with a plain icon instead of
  stacked cards with tinted badges and a check on every row. A tool call turns
  into a card only when you open its detail or it fails. The folded summary
  ("Ran 7 commands · 3 thoughts") is now a plain line with no border and no
  icon row; it shows a spinner while working and a stop mark if a call failed.
  Command rows drop the repeated `cd <your project> &&` and `export …`
  prefix from their one-line preview, so each row shows what actually ran
  (the full command is still in the opened detail). Finished thoughts fold to
  a single line with their first sentence; click to read the rest. The
  activity summary, tool rows and thought rows are now reachable with Tab and
  open or fold with Enter or Space, with a visible focus ring. Folded command
  rows are a single ink (syntax colors stay in the opened detail), file paths
  read from your project root instead of your home folder, finished thoughts
  no longer repeat "for 2 seconds", and a folded thought's caret now points
  right like every other folded row (it pointed down, which reads as open).
  The summary says how many calls failed ("· 1 failed") next to the stop
  mark, and an opened thought is set at the answer's text size.
  An opened file read is now just the code: no arguments block, syntax
  colors for the file's language, line numbers that start where the read
  started, indentation trimmed to the left edge, and one copy button. A
  partial read names its slice in the header ("lines 900–1029 of 1412")
  instead of printing pi's "use offset=… to continue" note as code.

- **Softer interface feedback.** Tooltips fade in and persistent update-notice
  actions use reversible hover transitions, with Reduce Motion respected.

- **Folder picker lists folders only.** The new-task folder picker now shows
  your recent folders (the current one first, then session folders and sidebar
  projects) and **Choose folder…** — nothing else. Worktrees, remote hosts and
  **Add this project to a server…** live only in the **Work in** picker, so
  choosing a folder no longer quietly changes where the task runs.

- **Send control motion.** The idle send button now grows into its label when a
  draft becomes sendable and reverses smoothly when cleared, respecting Reduce Motion.

- **Frosted composer cards.** Queue, retry, approval and inline-question cards
  now share the composer's floating glass treatment and accessibility fallback.

- **More readable frosted surfaces.** Without wallpaper, the floating glass tint
  now keeps secondary and muted text at 4.5:1 contrast over bright and dark backdrops.

- **Frosted settings and Usage boards.** Shared settings groups, empty resource
  cards and Usage boards use contrast-controlled dark glass where supported;
  full-window wallpaper settings and opaque fallbacks still apply.

- **Softer frosted workbench.** Shared cards, controls, fields and list rows
  use rounder corners, badges use a capsule shape, and floating surfaces have
  softer, broader shadows.
- **Frosted composer.** The composer now uses contrast-controlled glass on a
  supported dark workbench, including without a wallpaper, with an opaque fallback.
- **Floating surface hierarchy.** Dialogs have softer 16px corners, while
  pickers use the lighter popover elevation and shadow.

### Added

- **Inspector glass for tool calls and the detail docks.** Tool, thought, delegation,
  question and summary cards, and the Session details, Review and Explorer docks, now share
  one frosted material. They frost over a wallpaper as before and, in dark palettes, over the
  bare workbench too, with a tint held at the lowest alpha that keeps text readable over black
  or white. While frosted, an open tool or delegation card lifts with a quiet contact shadow; a tool call's command and output sit in a
  recessed well that keeps code legible over any backdrop. Light palettes, Reduce
  Transparency and Increase Contrast keep the opaque cards.
- **Worktrees in the Work in picker.** The picker now reads **Current checkout** (plus the
  worktree you are in, checked), **New worktree** — the Worktrees page's quick-create dialog —
  and **Existing worktrees ›**, which opens a flyout beside it with a search field over every
  worktree (branch over path). ↑/↓/Enter choose, → opens the flyout, Escape steps back. Worktrees
  now use their own mark (a folder with a branch, the Settings → Worktrees icon) in the picker,
  the Work in chip and the Worktrees page, instead of the branch glyph the branch chip uses.
  The new-task page's project selector offers the same places: **Current checkout**, every
  worktree as a row its search narrows (branch over path, the active one checked), and **New
  worktree** beside **New project**.
- **Activity and Usage in Session details.** The panel now counts what crossed the wire — prompts
  sent, replies received, tool calls and how many failed — from the transcript itself, and shows
  pi's running session totals: input, output, cache read/write tokens and cost. Rows appear only
  for figures pi reported. Values use tabular figures so a column lines up on its last digit.
- **Line counts on tool-call sections.** An expanded tool card's Command / Arguments / Output
  sections show how many lines they hold before you expand them.

### Fixed

- **The Git page's commit bar matches the working tree.** With nothing to
  commit, the bar is no longer an empty "leave blank to generate" message box
  with "Up to date" tucked into its corner: it is one row that leads with the
  sync status (up to date, behind, diverged) and trails with Push, Pull or
  Merge when one applies. The message editor comes back as soon as there is a
  change to commit. The tab strip is taller, so the selected tab no longer
  sits against the page card's rounded top corner.
- **Security boundaries are enforced end to end.** Opening a workspace no longer
  silently grants project trust: unknown or denied workspaces use `--no-approve`,
  keep project resources and MCP secrets off, and require an explicit persisted
  trust decision before those load. Subagents inherit trust and exact MCP secret
  names only in the same canonical workspace. Plan/Ask close shell mutation,
  MCP/custom-tool, invalid-mode, and resumed-delegation bypasses. Remote Markdown
  images no longer fetch automatically; avatar bodies are capped; clone URLs lose
  embedded credentials; quota adapters require exact official HTTPS origins. The
  updater now uses a private scratch root, extracts the same open file whose
  signature was verified, bounds feed reads, and avoids following error-marker
  symlinks.
- **Tool-call cards in the transcript.** A command's preview now fills the row and ends in an
  ellipsis at the card's edge instead of stopping at 72 characters. The ✓ and disclosure caret
  hold one column down the turn, with the edit card's copy button moved to their left. The
  **Thought** card's header uses the same badge, inset and height as the tool rows beside it, and
  its reasoning text is set in the UI face rather than the code font. The turn summary now reads
  "Read 3 files · Edited 1 file" instead of "Ran 3 file reads · Ran 1 file edit".
- **Tool-call and Thought cards drop their outline.** The tint and the frost now carry each
  card's shape, so a turn's cards read as one clean material instead of a stack of boxes. The
  Thought card renders its reasoning's Markdown (a `**Plan**` heading is bold, not wrapped in
  asterisks) and hangs the text on the title's edge, beside the badge.
- **In a session, Work in shows only where the task runs.** The status bar's Work in picker
  lists just the current checkout (or the worktree the session runs in). New worktree, Existing
  worktrees and remote hosts are offered on the new-task page, before there is a session to
  leave, and the "Choosing a folder starts a new task there" note is gone.
- **Pickers are calmer.** Rows in the project selector and the Work in picker now sit a small
  gap apart instead of touching, and only the highlighted row is filled — the current place is
  marked by its check and brighter text, not a second filled slab. The project selector drops
  the "This device" count, uses the same stroke icon as Work in instead of a shrunken project
  logo, and marks a remote host with a quiet muted "Experimental" tag instead of a warning
  badge.
- **Project selector: hosts get their own group; trailing items sit on the title line.** A
  remote host no longer appears under "This device" — hosts open a **Remote hosts** section of
  their own. A row's branch note, check, count, `prunable` tag or "Experimental" badge now rides
  its title's line in the project selector and the Work in picker, instead of floating between
  the title and the path. Paths under home read `~/…`, as in the Work in picker.
- **Pickers: one footer, one edge, nothing cut off for good.** The Work in picker's note
  ("Choosing a folder starts a new task there") sat 6px left of the rows and header; it now
  shares their edge inside a single footer zone with the key legend, and the rewind picker uses
  the same footer instead of stacking two rules. Every picker row (Work in, folder, rewind,
  model, server clone, provider hosts) now shows its full title and path on hover, so a long
  branch, path or `user@host:dir` is never cut off with no way to read it. The Work in picker's
  width follows the UI font size.
- **Card corners and lift are consistent.** Every inspector card's header, error strip and
  detail plane round to the card's radius less its border, so inner corners nest inside the
  outer ones; an open delegation card now lifts like an open tool card; and the tool sections'
  line count uses the regular muted ink instead of a dimmed copy of it.
- **Tool-call sections: Copy no longer covers the label.** The section's Copy text sat floated over
  the label row; it now has its own slot, so a long or translated label truncates before it. The
  image grid shares the code sections' 12px inset.
- **Session details holds up at large text and in other languages.** Rows grow with the UI font
  instead of clipping at a fixed height, the Changes row wraps its Review / Commit actions, and the
  session id and file show in full in the code face rather than losing their tail to an ellipsis.
- **A sent message's file chip no longer lights up on hover.** It has nothing to open or remove,
  so it now renders as a read-only `FileChip` and gives no hover response.

### Changed

- **One owner for icon tiles and file chips.** The transcript's activity badge and the pickers'
  and Usage legend's leading tile are now `ui::glyph::icon_tile` (`TileLook::Tinted` / `Raised`);
  the tool card's truncation mark is a `Pill`; a sent message's file chip is a `FileChip`.
  `scripts/check_ui_owners.py` now guards `icon_tile` and `TileLook`.

## [0.2.20] - 2026-11-20

### Added

- **Live subagent thinking and tool output in the right pane.** The subagent tab now streams
  the child's reasoning in a **Thinking** section (marked *live* while it thinks; the newest part
  of it) and shows what each step returned — the first lines, live while a long call runs —
  under the step. The bundled subagents extension sends both as bounded, optional fields of the
  existing `subagent/1` payload (reasoning ≤ 4,000 characters, each output ≤ 1,200), so older
  Orbit builds keep their card. Neither reaches the parent model.
- **Desktop background: a frosted-glass window.** Settings → Appearance → Background has a
  fourth placement, **Desktop**: the window turns see-through and the system frosts the desktop
  behind it (macOS vibrancy, Windows Acrylic), with Orbit's panes translucent on top. **Card
  opacity** sets how much paint sits over the frost, but text-bearing surfaces never go clearer
  than keeps them readable over a black or a white desktop; **Glass panes** still frosts the
  workbench behind menus and the composer. It needs a dark palette and Reduce Transparency off,
  and says so when either is missing; light palettes and Linux keep a solid window.
- **Subagents in the right pane.** A delegated child's log on its card has **Open in side
  pane**, which opens that child as a tab in the right pane beside **Review**: its agent,
  status, elapsed time, tool count, tokens and cost up top, a live "now" line while it works,
  then its error and blocked calls as banners, the full task, its setup and tools as badges,
  its steps and its report — each in an inset well that sits on the pane's frosted glass over
  a wallpaper or the Desktop background. Tabs belong to their session — switching away and
  back restores them, and never pops open a pane you hid. Closing the last subagent tab
  leaves Review.
- **Double- and triple-click selection in the transcript.** Clicks cycle the way Pi's TUI
  does: a second click on the same word selects it, a third selects the visual (wrapped)
  line, and a fourth goes back to a caret. Words follow Pi's rules — `/ - + \ ~ _ .` join
  their neighbors, so `unity-codegraph`, `file_name.rs` and paths select whole, while each
  Han character is its own word. Dragging after a double- or triple-click grows the
  selection by words or lines, a click past the end of a wrapped row stays on that row, and
  a word or line click on a link does not open it.
- **Star Orbit from Settings → About.** A **Star** button sits beside the GitHub link. When
  the GitHub CLI is installed and signed in, it stars or unstars the repository with your
  `gh` account (`gh api /user/starred/…`), on click only, and reads **Starred** once you have.
  Orbit checks the current star once per launch, the first time About opens. Without a usable
  `gh` (missing, signed out, or a token without the scope) the same button opens the
  repository in the browser.
- **The server's branch on remote sessions.** The header and status bar show the branch (and
  ahead/behind) of the session's folder **on the server** — never this Mac's — in the same
  branch selector as local sessions: search, the server's branches newest first, and
  **Create and checkout new branch…**. Picking or creating a branch switches it on the server
  over ssh; git's own error is shown if it refuses (for example over uncommitted changes).
- **Sidebar "By date" view.** The sidebar view menu has a new **By date**
  grouping: one list under day sections (Pinned, Today, Yesterday, This week,
  This month, Older), counted in local calendar days and newest first inside
  each section. Pinned sessions keep their own section on top, so no session
  is listed twice. The choice is remembered across restarts.

- **Your backgrounds.** Settings → Appearance → Background now lists the
  pictures in your own folder, `~/.orbit-pi/wallpapers/library/`, next to the
  built-in ones; **Open folder** opens it, and pictures you add or remove show
  up while Settings is open. Orbit only reads the folder, so your pictures stay
  on your computer.

- **Pull requests and checks on a commit.** Opening a commit on the Git
  page's History or Graph tab now shows the pull request it belongs to (who
  opened it, who merged it — click to open it on the Pull requests tab) and
  its CI checks, with failing and running checks listed by name. The card is
  also tidier: labeled Author / Committer / Commit / Parents rows with
  relative times (exact time on hover), and a file-change total. Needs a
  GitHub remote and a signed-in `gh`; one request per commit you open.

- **Initialize Git from the Git page.** Opening Git in a folder that is not a
  repository now says so and offers **Initialize Git repository**, instead of
  showing empty tabs. It runs `git init` with your own default branch name.

- **Git details for remote sessions.** On a remote session the Git page shows
  the host's own Git state for the session's folder — repository root, branch,
  upstream with ahead/behind, changed files, last commit and `origin` (any
  credentials removed) — read over SSH without changing anything. A folder
  that is not a repository, or a host without `git`, is said plainly.

- **Remote sessions in their project's group.** A remote host entry can now be
  linked to a project on this computer (Settings → Remote hosts → **Local
  project folder**). With the sidebar grouped by project, that host's sessions
  list inside the project's group, after its local sessions, instead of in a
  group of their own at the bottom. **Add this project to a server…** links
  the entry it saves to the project it cloned.

- **Built-in backgrounds.** Settings → Appearance → Background now offers
  one-click backgrounds in four rows: Gradients (seven generated gradients),
  Nature, Anime (original anime-style illustrations) and Japanese art (woodblock
  prints by Hokusai and Hiroshige). The images ship with the app and are
  credited in `assets/wallpapers/CREDITS.md` and in each thumbnail's tooltip.
  They work like a picked image: placement, blur, effects, glass and
  accent-from-image all apply.

- **Dithered Hero wallpaper preset.** Apply fine dither, no image blur, and a
  bottom fade to your chosen image on the new-task screen. The preset is opt-in
  under Settings → Appearance → Background. The separate **Bottom fade** toggle
  also blends full-window wallpaper into the active theme at the bottom edge.

- **Frosted glass pages.** With a full-window wallpaper, the Git, Usage, Files and
  Worktrees pages, the expanded terminal, and the Review, Explorer and session-details
  docks are now frosted glass cards: the picture blurs softly behind them, their inner
  cards and headers are translucent on the glass, and the Review diff's sticky headers
  frost the code scrolling under them. Previously each page painted an opaque canvas over
  its card, so the wallpaper never showed through. Hero wallpaper and no wallpaper keep
  the opaque pages.

- **Remote sessions in the sidebar.** Each enabled remote host entry gets a group under your
  projects listing the sessions that ran in its folder on that server, newest first; clicking
  one resumes it **on the server** (the session's own model and Plan/Build/Ask mode kept). The
  server lists its own pi store over ssh — read-only, only each row's title, folder and time,
  never a transcript. Lists refresh at launch, on ⌘R and after a remote turn settles without
  asking for anything; a server that wants a password says so, and its group's refresh button
  signs in.
- **Focus comes back after more dialogs close.** The MCP server editor, the remote
  host editor, the provider account editor, the remote API-key prompt, "Add to a
  server" and the SSH password prompt now return the keyboard to whatever opened
  them when they close — by button, Escape, the scrim or a finished operation —
  instead of leaving focus nowhere. While they are open, **Tab** and **Shift-Tab** move
  between their fields and wrap, instead of doing nothing. An extension's question
  dialog labels itself in your interface language instead of always in English. Menus that close from an outside click now
  all ignore that same click's release on their trigger for the same 200 ms.

- **Home / End in long pickers, and Retry when Review cannot load.** The worktree
  "Work in" menu and an extension's select dialog jump to their first and last
  rows with **Home** and **End**. When the Review pane fails to read a diff, its
  error now carries a **Retry** button that runs the same reload as the header's
  refresh.
- **One button and badge vocabulary across the app.** Buttons now come from one
  component with named variants (primary, secondary, outline, tinted, danger,
  ghost), so the same meaning looks the same in Settings, Subagents, MCP, Git,
  Worktrees, Usage and the updater. A few buttons changed slightly as a result:
  primary buttons gain the 1px edge their neighbours already had (2px wider),
  primary buttons press by colour instead of also dimming, disabled buttons use
  the shared disabled frame, and the session-retention, plugin and runtime
  buttons take the shared medium weight. Provider badges are 18px tall like
  Git's ref badges. Inline warning and error notices share one banner, so the
  Worktrees error and the inactive-subagents notice match the app's other
  error cards.

- **Frosted floating surfaces without a wallpaper.** In dark palettes on macOS,
  menus, pickers, popovers, tooltips, notifications, modals and the
  jump-to-latest button now frost the workbench behind them, a 24 pt blur
  under a tint of the card colour, instead of painting a solid card. Each
  palette gets the lightest tint its text allows: the lowest opacity (never
  below 55%) at which text keeps its contrast even over a pure white backdrop,
  so Onyx shows more glass than Flexoki.
  Light palettes keep their solid cards. macOS **Reduce Transparency** or
  **Increase Contrast** brings the solid card back everywhere, and over a
  wallpaper, Settings → Appearance → Glass panes still decides. The sidebar,
  panes and the composer frost only over a wallpaper, as before.

- **Reordering projects and sessions in the sidebar.** Drag a project header onto
  another one and a line shows exactly where it will land; release, and the order is
  yours. Session rows rearrange the same way inside their own project (a session never
  changes folder), newest-created tasks still landing at the top of the group. With the
  sidebar focused, **⌥↑ / ⌥↓** move the row the cursor is on — a project header or a
  session — and both row menus list **Move up** / **Move down** (shown disabled with the
  reason at either end). A project move is applied to the order on screen and pins the
  project sort to **Manual (added order)**, with a toast when that changes, so the new
  order stays put instead of being re-sorted by activity on the next repaint.
- **More than one account per provider, all usable at once.** Settings → Providers &
  models has an **Accounts** panel in every provider's row: add, rename or remove an
  extra account and see whether each one is signed in. Every account is its own
  provider row (`ChatGPT (Codex) · Work`) with its own sign-in — browser, device code,
  pasted code or API key — and all accounts are live in every session at the same
  time: pick any account's model in the model picker, no switching and no restart.
  A bundled pi extension registers each account as a copy of its provider under its
  own id, so pi stores and refreshes every account's credential separately in its own
  `auth.json`. Stores from the earlier per-session design migrate automatically (each
  sign-in moves to its account; nothing is copied). Remote sessions keep the host's
  own credentials.
- **Add a project to a server.** Like t3code's "Add to another machine": the new-task
  folder picker's **Add this project to a server…** (or a host card's **Add a project from
  this Mac…**) clones the project's `origin` with git **on the server** — SSH or HTTPS, any
  token in the URL stripped — into a folder you choose (`~/<repo>` by default), then copies
  the ignored files you tick (`.env`, `.env.*`, `.envrc`, `.dev.vars` ticked by default; up
  to 1 MB each) over ssh's stdin, written 0600 and kept inside the clone. Clone failures are
  explained (git missing, git host key not trusted, key refused, HTTPS login needed), a
  host entry is saved for the new folder, and a session starts there.
- **Remote sessions now respect Plan / Build / Ask, the default model and the access mode.**
  The workflow extension is installed on the host beside the access guard, the session
  starts in the chosen mode (`ORBIT_WORKFLOW_MODE`), and a mid-session change is written
  to the host's `~/.orbit-pi/workflow.json` over ssh — a failed write says so. A new remote
  task applies the default model through RPC when the host's own catalog has it. The
  access chip shows Supervised while remote (what the host is pinned to) and its menu
  stays shut with the reason. Non-image file attachments are refused with a toast instead
  of sending a path that does not exist on the host.
- **Run the agent on a VPS over SSH.** Settings → Remote hosts manages the machines,
  after t3code's environment panel: **Add host** opens a form for the SSH host or
  `~/.ssh/config` alias (your aliases are offered as suggestions), an optional user and
  port, the remote directory and a display name, and **Test connection** checks — without
  changing anything on the host — that ssh connects, the directory exists and `pi` is
  installed, before you save. Each host row shows `user@host:port`, the directory and its
  last check, with Test, Edit and Remove. Hosts are stored in
  `~/.orbit-pi/remote-hosts.json`, written surgically so hand edits survive. The new-task
  folder picker lists the hosts, and **Add a remote host…** opens the form. Picking
  a host spawns `pi --mode rpc` there over `ssh`; the RPC stream rides ssh's stdio, so
  no server, port forward or pairing is involved. Agent only: Files, Git, Review,
  Worktrees, the Explorer and `@` mentions read this machine's disk and are off for a
  remote session, which the folder chip and status bar name by host. The access guard is
  installed on the host on every launch and the host's mode is pinned to Supervised; a
  session never starts without it. Connections try SSH keys first; when the server refuses
  them and accepts a password, Orbit asks for it in a masked dialog and hands it to that
  one ssh process through an askpass helper — kept in memory until Orbit quits, never
  written to disk, up to three tries. Unknown host keys are verified in a terminal, never
  trusted from the app. Password logins need macOS or Linux; keys work everywhere.
  Each host is a card (name, `SSH user@host:port · status`, an on/off switch that hides
  it from the folder picker, and a ⋯ menu with Test connection / Edit / Remove). A failed
  check says why — a cloud image that only allows `opc`/`ubuntu` (**Use opc**), a login
  profile stopped by a signal, or pi missing — and **Install pi** installs it over ssh
  with pi's official installer, plus checksum-verified Node.js 22 from nodejs.org when the
  server has none; no sudo, no profile edits. Orbit also finds pi through your own login
  shell and in pi's usual install folders, as t3code does for its agent CLIs.
  During a remote session, Settings → Providers → **Sign in** runs `pi /login` on the
  server in Terminal (over `ssh -t`, so ssh and pi ask their own questions there), and
  **Restart pi** reattaches to the server's session so the model picker lists the newly
  signed-in provider. The page says which machine each control acts on.
  Providers & models has a **Machine** selector (This Mac, plus each enabled remote host,
  after t3code's environment picker). A host shows its own signed-in providers and their
  models, read from that server's `pi --list-models` over ssh so credentials never leave
  it, with **Sign in** (pi's own `/login` on the server: OAuth and API keys) and **Refresh**.
  The page opens on the machine the current session runs on. On a host, **Add API key**
  saves a key there without leaving Orbit (typed into a masked field, or **Use this Mac's
  key**); it travels on ssh's stdin only and is merged into the server's `auth.json`
  (0600). Subscription sign-ins open pi's `/login` for the server in a tab of Orbit's own
  terminal panel. OAuth credentials are never copied between machines.
  Restarts and reconnects stay on the host; leaving a remote session stops its process,
  and remote sessions are not listed in the sidebar (pi keeps their files on the host).
- **New project without starting a task.** **New project…** in the new-task folder
  picker (and in the command palette) asks for a folder and adds it to the sidebar as an
  empty project group, whose own `+` starts a task there. Nothing is written to pi, and a
  folder that is already listed says so instead of appearing twice (#131).
- **Delete project & files.** The sidebar project menu now separates the two meanings:
  **Delete project** drops the row from the sidebar (pi's sessions untouched, as before),
  and **Delete project & files** asks once, naming the folder, before removing it from
  disk — to the Trash on macOS, permanently where the platform has no recoverable delete
  (the confirmation says which). Guards refuse a filesystem root, the home folder, a
  folder that contains another project, and a folder a live task is still working in; the
  row only leaves the sidebar once the folder is actually gone (#131).
- **The terminal opens as a page.** The bottom panel's header carries an expand
  control beside Restart: one click and the shell leaves its strip to own the main
  column the way the Git page and Usage do — chat skipped, tab strip and controls at
  the top, the grid filling everything below, a **Back** and the page title in the
  shared top bar. The same control (now a shrink glyph) puts the strip back. The
  docks keep their place, since a page is the column, not the window, and the panel
  reopens docked after it is closed — ⌘J never changes two things at once.

### Fixed

- **An opened session no longer jumps back to the bottom.** Opening a session paints pi's
  on-disk copy and then pi's live `get_messages` reply. When both hold the same rows, the
  transcript now re-measures them in place instead of rebuilding the list, which had thrown
  away every row height and snapped a reader who had scrolled up back to the latest message.
- **Mouse-wheel scrolling in the transcript moves from where you are.** Each wheel event now
  moves the list's current position (the vendored GPUI list used to replay the whole gesture
  onto the position from its first paint), and a single notch can leave the live edge.
- **Issue and pull request rows show their title and labels again.** A list
  longer than the pane squeezed every row down to its first line, so each row
  read only "Issue · #154" with the time. Rows now keep their full height and
  the list scrolls.

- **The Remote hosts card's ⋯ menu closes when ⋯ is pressed again.** The press
  first dismissed the menu as an outside click and its release re-opened it, so
  the menu could only be closed by clicking elsewhere. The menu is also sized to
  its labels now, so "Add a project from this Mac…" is no longer cut off.

- **Fullscreen is remembered across launches.** Closing or quitting while the main
  window is in native fullscreen now reopens it in fullscreen, not merely maximized.
  The ordinary window frame is still remembered and restored when leaving fullscreen;
  maximized remains a separate state (#131).

### Changed

- **Questions and choices read as a decision card.** The agent's questionnaire panel and an
  extension's plain `select` now share one card above the composer: a *Decision needed*
  header, the question in a larger weight, and one row per option with a radio (or a check for
  multi-select), its description always visible, and a **Recommended** badge where the label
  carries `(Recommended)`. A `select` no longer dims the window behind a modal; confirm, input
  and editor dialogs keep the modal. ⏎ / click answer, esc or × cancels.
- **Subagent card and pane read in full.** In the side pane a child's time, tool count, tokens
  and cost wrap onto a second line instead of being cut off at the edge, and its current activity
  wraps instead of clipping. A running child says *running a tool* or *blocked* when that is what
  it is doing, not always *thinking*. On the card a long agent name truncates before the counts
  and chevron are pushed out, and a finished child's check is green, as it is in the pane.
- **Queue and Steer are named on the send controls.** Mid-run with something typed, the
  primary send keeps its label (**Queue** or **Steer**) and the other lane is a labelled pill
  beside it, instead of two look-alike arrow discs. Queued rows tag their lane with a matching
  badge and glyph.

- **A quieter About page.** Settings → About opens on the app's identity — its mark beside
  the name, the one-line description and a single "Version · GPUI" line — over one board:
  Updates, the pi agent's connection, and Source with **Star** beside **GitHub**. The star
  card and the separate Orbit Pi and GPUI rows are gone; their facts moved into the header.
- **Quieter chrome.** Top-bar controls (quota, details, open-in, review,
  terminal, explorer) no longer sit in framed chips. They rest as plain icons
  and text and pick up their glass only on hover or while their panel is
  open. The provider-quota meter shows the provider's mark only (its name is
  in the tooltip and the popover). The model and
  thinking-level chips now read as one setting ("Opus 5.5 Medium ▾"): the
  level loses its icon and sits in muted ink after the model name, and each
  half still opens its own picker. The composer placeholder now mentions `@`
  files and `/` commands. Both meters stay neutral until they need attention:
  the context meter turns amber at 70% and red at 90%, and the quota chip
  turns amber at 75% and red at 90%.

- **A calmer transcript.** The "Worked for 20s" row at the head of a settled
  turn is now a quiet left-aligned disclosure instead of a centred divider
  with rules on both sides. Under each answer, the footer shows only tokens
  in and out (`↑141.4K ↓24.7K`); cache share, cost and output rate are in
  its hover card, which already listed them. The end-of-task changed-files
  summary is a borderless tonal panel with a one-line header ("Changed 3
  files +247 −104" and Review) instead of a framed card with an icon tile and
  rules between its sections. It still shows three files, then "Show N more".

- **Quieter session rows.** The branch icon on a session row (and in its
  hover card) is monochrome, and ahead/behind counts show on the open
  session's row only; every row's hover card still lists them. The sidebar's
  section label shows how many sessions it lists.

- **Remote hosts are marked Experimental.** Settings → Remote hosts, its
  navigation entry and each server in the new-task folder picker carry an
  *Experimental* tag, and the page says what that means: remote sessions may
  misbehave or change between releases.

- **The new-task dither field actually moves.** The dots in the background
  behind a fresh session used to drift one noise cell every 29 seconds and be
  re-rastered five times a second, so most frames came out identical and the
  field read as a still image. It now drifts a cell every 2.5 seconds at 10 Hz
  — the idle heartbeat's own ceiling — on a finer 7 px ruling with a slightly
  wider density band, so the grain visibly flows without the grain catching up
  with the watermark or the page leaving the theme's canvas. Reduce Motion still
  holds the frame it was showing.

- **Your messages and terminal output on glass.** With a wallpaper and Glass
  on, the bubble around your own messages is now frosted glass like the tool
  cards beside it, and a tool card's command and output pane no longer covers
  the card's frost with a solid block. Thinking cards now use the same frosted
  card as tool cards, instead of a lighter wash that made the two look like
  different materials. Without glass, everything looks as it did before.

- **Subtle hover on glass.** Outlined chips (such as the transcript's
  "1 thought" chip) and Secondary and Outline buttons no longer turn
  see-through under the pointer over a wallpaper or a frosted menu; their face
  lifts one subtle step and stays solid.

- **A cleaner Theme section.** Light / Dark / System is now a small segmented
  control, and the dark theme, light theme and accent are compact dropdowns.
  Each theme option shows a small swatch of that theme in your accent, and each
  accent option a dot of its colour. The large picture cards and thumbnail
  galleries are gone.

- **Selected and hovered rows on glass.** Over a wallpaper, and inside frosted
  menus, pickers, popovers and dialogs in dark palettes, the open row, the
  chosen option, an open chip or icon button and the active tab now use a
  translucent wash instead of an opaque block, so the glass shows through the
  state. Info, success and warning banners and Usage's loading bars are
  translucent on glass pages too. Opaque surfaces look exactly as before.

- **One component layer for the whole app.** Every button, chip, banner, field,
  menu, popover, modal, row, table, tree, tab strip, diff line, status bar and
  rail now comes from one shared component, so they look and behave the same on
  every screen. Visible effects: segmented controls share one cross-fading
  style with medium labels; icon buttons press with the same wash; the Git
  failure notice, the worktree setup banner and the Explorer's notice strip use
  the shared banner; the Review tree's keyboard cursor matches the Explorer's;
  transcript edit diffs use Review's red/green strengths; dialog headers share
  one layout. Settings → Appearance theme previews no longer show square corners
  inside their rounded cards.
- **Context menus work from the keyboard.** The Explorer, transcript text and
  sidebar session/project menus take the arrow keys, Home/End, Enter and
  Escape; Shift-F10 or the Menu key opens a sidebar row's menu (and the
  Explorer's). Form dialogs' Cancel and Save buttons are Tab stops — Enter or
  Space presses them.
- **The wallpaper's page tint is yours, not the contrast pass's.** In full-window
  mode the sidebar, the chat column and the settings nav used to take at least
  65% of the palette's own background colour over the picture, and the contrast
  pass could raise that further on a bright image — so a chosen wallpaper read as
  a black-tinted backdrop however translucent you asked for. **Card opacity** now
  paints those panes at exactly the value you set, all the way down to 0%, where
  the image is shown as processed with no tint at all. Cards, the composer,
  menus and dialogs keep their contrast pass, so their text stays readable.
  Dimming the picture is now only **Image opacity** (plus **Glass panes** for the
  panes themselves, which blur without tinting).

- **One design system, and a redesigned Theme and Background.** The
  Classic / Operational switch is gone: Operational and Operational Light are
  now ordinary palettes beside the Orbit family, Flexoki and Onyx. Settings →
  Appearance → **Theme** shows Light, Dark and System as picture cards, then a
  **Dark themes** and a **Light themes** gallery of thumbnails, each a
  miniature workbench painted from that palette with your accent; each gallery
  remembers its own choice and the one on screen is marked **In use**. Accent
  colour is a row of swatches instead of a dropdown. **Background** leads with
  a larger preview that, with no image, is an empty state with **Choose
  image…**; placement is a segmented control, and the sliders and pane toggles
  move to their own **Image adjustments** board once an image exists. Language
  has its own section. Every picker is one tab stop, with arrow keys moving
  inside it. If you had chosen the Operational design system, Orbit opens on
  the Operational palettes as before.

- **Toasts restyled after shadcn's toast.** Cards are cleaner: an inline status
  glyph instead of a tinted tile, a medium title over a muted description at
  the same size, roomier padding and a quiet × centred on the right. Several
  toasts now rest as a collapsed deck, the newest in front and up to two older
  ones peeking above it, and fan out into a column under the pointer. While
  the pointer is over the stack nothing expires; leaving resumes each card's
  timer where it stopped. New cards rise into place as they fade in; with
  Reduce Motion on they simply appear.

- **Settings → Subagents reads as one workflow.** The agent header is a single
  bar — name and status on the left, Duplicate, Save and the Enabled switch on
  the right — and the form follows the order an agent is written in: identity,
  prompt, tools, mode, model, file, then the read-only preview. Delegation
  limits apply to every agent, so they moved off each agent's form to their own
  page, opened from a pinned row above the agent list; the open agent's edits
  survive the trip. The form stops widening at 760 px.

- **Sidebar Projects panel polish.** Sessions under an expanded project now step in
  under the project mark, with a hairline tree guide down the chevron column, so the
  project → session hierarchy reads at a glance; the "Show N more" / "Show less" rows
  sit on the same indent and guide. Project marks are one consistent tile (logos fill
  it, glyphs sit centred on it, both behind the same hairline). Session counts are
  plain muted figures instead of filled chips, the preview line is muted beneath a
  tighter title, and the section label is a small eyebrow with a rule to the edge.
- **The UI got its own layer.** `crates/orbit-pi/src/ui/` now sits between the design
  tokens and the surfaces, which is where every element will move as it gains one named
  owner (`docs/plans/ui-component-layer.md`). This first landing is a pure move:
  `TipExt`, `ControlExt`, `StatefulControlExt`, `PressedRamp` and `AccentWash` live in
  `ui/style/control.rs` (re-exported, so no call site changed), the tooltip view moved
  from `usage/tooltip.rs` to `ui/tooltip.rs` because a shared element was living under a
  page, and `scripts/check_ui_layers.py` now fails the build if `ui/` imports a surface
  or re-derives the backdrop-blur marker outside `ui/style/surface.rs`. Rendering is
  unchanged.
- **Settings → Providers & models is a list, not a card grid.** The three-column grid
  squeezed each provider card to ~200px, so facts, buttons and model lists clipped on
  ordinary window sizes. Providers are now rows on the same grouped boards the other
  settings pages use (Connected / Not connected, with counts). A row is one calm line —
  brand mark, name and status over a line of facts, and on the right only the model
  count, a **Sign in** / **Add API key** call to action while the provider is not
  connected, and a chevron. Clicking the row opens its panel: every action (each sign-in
  method, Models/Configure, Usage, Disconnect, Remove) on a full-width bar that wraps
  inside the row, then the provider's models as a two-column, height-bounded list of
  click-to-show chips. The panel opens on its own for the provider of the session's
  model, while a search or the Favorites filter narrows the list, and while a sign-in
  runs. Search and the page's buttons share one row, and the active-model selector sits
  on the shared board.

- **The Git page (GitHub) button moved from the top bar's "…" overflow menu to
  the left sidebar's footer**, ahead of the Usage button, where it sits as a
  workbench destination (⌘⇧G unchanged). The top bar keeps only live-state
  controls: quota, session details, open-in, Review, Terminal, and "…" for the
  Explorer.
- **The Explorer has its own top-bar button, and the project panel is redesigned.**
  The "…" overflow menu held only the Explorer toggle, so it is gone: a folder
  button after the Terminal chip toggles the project panel (⌘⇧E), lifting while
  it is open. The panel's header now names the workspace over its `~/` path
  (full path on hover) with a spinner while the tree loads, and keeps New File,
  New Folder and Close visible; Refresh, **Collapse All Folders** and the
  hidden-files toggle moved into a "…" menu beside them. The filter is an inset,
  rounded field whose border takes the focus ring, rows draw indent guides
  through each ancestor's chevron so nesting reads at a glance, and the file
  count sits in a quieter footer.
- **The Terminal toggle has its own top-bar chip.** ⌘J's control left the "…"
  overflow menu and now sits directly after the Review chip, lifting while the
  panel is open — the bottom panel is one click away, like Review, instead of
  two.
- **Session details is a side panel with its own top-bar chip.** The `ⓘ` row
  left the "…" overflow menu and now sits immediately after the usage meter.
  It no longer drops a popover: it opens the session's details in the right
  dock beside the chat, laid out as an inspector with a close button in its
  header. **Session** shows the title as text with an edit (pencil) button and Generate
  title; editing swaps in a compact rename field with **Update** and **Cancel**
  (Enter commits, Escape cancels), sized from the same tokens as the rest of
  the panel so it follows the UI font size;
  **Environment** names the machine (this computer, or the remote host), the
  folder and the branch with its ↑/↓ divergence; **Model** shows the provider
  mark, model and thinking level; **Context** shows a fill bar, the percentage
  and tokens used of the window (or "No usage yet" before pi measures any);
  **Changes** shows the working tree's `+N −N` with **Review** and, when there
  is something to commit, **Commit**; **Details** keeps the copyable session id
  and file. The same chip closes it; an outside
  click does not, and Escape only hands focus back from the rename field. The
  panel shares the dock with Review and the Explorer, one at a time, and the
  palette's Rename Session opens it. A new task has no session to describe, so the chip and the panel stay hidden on the new-task page until the first message; a panel left open comes back with it.
- **The folder picker is a machine picker.** The two places that ask "where does
  this task run?" — the new-task composer's folder chip and a session's status
  bar **Work in** chip — now list two kinds of place: **this device** (the current
  working folder) and each configured **remote host** from Settings → Remote hosts.
  Linked worktrees, the eight recent session folders and the new / existing /
  remote-branch verbs no longer crowd the list. Worktrees keep their own page
  (which now also carries **Clone repository**, since cloning left the picker),
  and the new-task picker still has **Choose folder…** for the native dialog.
  The picker's **Add a remote host…** row is gone — host setup lives in
  Settings → Remote hosts, reached from the **Work in** menu. The device row
  now leads with the workspace's own logo (the same mark the header's folder
  chip shows) and carries the active branch as its trailing note, and the
  section header reads **This device**.
- **Remote hosts are reachable from an open session.** The status bar's **Work in**
  picker reads `~/.orbit-pi/remote-hosts.json` when it opens, so a running session
  can move to a host without going back to the new-task page; the connection check,
  password prompt and guarded launch are the same path the picker uses.
- **Providers & models: one grid, with each provider's models on its own card.**
  Connected and unconnected providers are the same three-column cards, so a connected
  provider is no taller than a disconnected one, and every provider that pi reports
  models for lists them **on its card** as click-to-select chips: a chip is selected when
  the composer's picker offers that model, one click hides or shows it, each shows its
  context window, and **Hide all / Show all** sits beside the count. Choosing never
  deletes anything — the provider's catalog (pi's, or `models.json`) is untouched, so a
  model the provider adds later still arrives. The card's **Models**/**Configure** button
  still opens the ids, endpoint and key editor (ids stay editable there as the bulk-paste
  path, and after **Fetch models** the fetched ids are added to them); it no longer
  repeats the list, and a provider pi reports nothing for shows no list at all (#131).

### Fixed

- **No focus ring on the selected Settings section.** Clicking a section in
  the Settings list left a focus outline around it on top of its selected
  fill. The ring now shows only when the keyboard cursor rests on a
  different row, where it marks the row Enter will open.

- **Remote host menu opens under its button.** The "…" menu on a Settings →
  Remote hosts card opened about 100 px to the left of its button and above
  it, over the card. It now hangs right-aligned under the button, like every
  other dropdown.

- **Settings → Remote hosts no longer crashes with Reduce Motion on.** The
  Add host button, each card's "…" button and the host editor's Cancel/Save
  had their hover set twice, which GPUI rejects when Reduce Motion is on.
  They now use the buttons' own fade.

- **Navigate Settings without a mouse.** Tab reaches Back and the section
  list. Arrow keys move focus without switching pages; Enter or Space opens
  the focused section. Back and Escape return focus to the previous control.

- **Extension confirmations default to No.** Enter answers No when the
  dialog opens, even if the pointer is over Yes. Tab moves between the
  buttons without answering.

- **The folder selector no longer nudges its row.** Closing the new-task
  folder picker (or tabbing onto any button) moved the folder name and the
  branch chip 1–2 px, because the focus ring added a border the button did
  not have at rest. Buttons now keep that 1 px edge transparent until they
  are focused, so focus only changes its colour.

- **Solid blocks over the wallpaper glass.** With a full-window wallpaper,
  the sidebar's sticky project header, the Settings → Skills and Subagents
  pages, the boxes inside provider, account and MCP cards, the About page's
  star banner and the Usage page's loading placeholders now take the same
  frosted or tinted material as the surfaces around them instead of painting
  opaque panels. Without a wallpaper nothing changes.

- **Dialogs and overlays now match.** Settings dialogs (provider usage and
  key, account, MCP server) dim the window as much as every other dialog
  instead of a darker hand-mixed shade; the Git page's confirmations dim it
  the same way the command palette does. The sidebar's drag preview, the
  composer's second send button and the empty-state cards in Settings
  (no plugins, no matching providers or settings) now take the same glass
  as the surfaces around them.

- **One look for errors and notices.** Error messages in dialogs (provider
  key and editor, accounts, SSH password, remote hosts, MCP server, server
  clone, bug report) share one tint, a failed turn in the transcript shows
  the standard error notice, and a merge or rebase in progress on the Git
  page uses the standard warning notice, as does the branch menu's
  uncommitted-changes warning. The MCP server switch and the
  Settings sidebar toggle are now the shared switch, so Tab reaches them and
  Space or Enter flips them. The open Settings section now looks selected
  rather than hovered.

- **Commit details showed the wrong values.** An opened commit on the Git
  page listed its committer date as **Parents** and folded its branch and
  tag names into the message, because one field was read out of order. The
  open commit and its details now also read as one card instead of two
  stacked boxes.

- **Dialogs grow to fit what they hold.** The Add account dialog on Models &
  providers cut its Cancel and Create account buttons off at the bottom, and the
  API key, provider, usage, MCP, remote host, remote API key, "Add to a server",
  SSH password, extension question and Git branch / stage dialogs could come out
  shorter than their content, scrolling a body that would have fit. Each dialog
  now takes its full height and scrolls only when the window is too short for it.

- **Subagents detail sections no longer collapse.** The detail pane's scroll
  body had no definite width, so each board shrank to its content and the
  delegation-limit labels ran under their steppers.

- **Sidebar previews of skill prompts:** a session started with `/skill:name` showed
  the raw `<skill name="…" location="/Users/…">` wrapper as its preview (and as its
  title until pi named it). It now reads `/skill:name` plus what you typed, the same
  form the transcript shows. The "Show N more" / "Show less" rows now draw the
  keyboard focus ring they were already asking for.
- **Providers page:** every provider row's details line painted as a bare "…"; it
  reads in full again. Sign-in buttons on unconnected providers are secondary
  buttons instead of a column of filled ones.
- Tests no longer write fixture session bindings into the real `~/.orbit-pi`
  account store, and two test fixtures no longer collide when tests run in parallel.

- **The Explorer no longer slides off the window as files open.** The Files,
  Git, Usage and Worktrees pages could not shrink below their content's width,
  so a long run of open file tabs pushed the project panel past the right edge
  (its Close button out of reach). The page now yields and its tab strip
  scrolls, keeping the panel at its width against the edge. The panel's minimum
  width is now 220 px — enough for the redesigned header — and a narrower saved
  width opens at that minimum. The header's path line now shows the path
  instead of a lone "…".

- **Double-clicking the header now zooms the window.** Orbit paints its own titlebar,
  so the press never reached AppKit's titlebar view and the system's
  Double-click-a-window's-title-bar action did nothing. The header strips (the sidebar
  wordmark, the top bar's drag spacer, the Settings header) now ask the platform to run
  that action on a double-click — zoom, minimise, fill, or nothing, exactly as
  System Settings → Desktop & Dock says — while a single press still just drags.
- **Delete session no longer disappears from the task menu.** It was hidden for the task
  that was open or running, which read as a missing feature; the row is now always
  listed and, while pi's process still owns the file, disabled with the reason on hover.
  **Reveal in Finder** — a macOS-only label shown on every platform — is gone from that
  menu (#131).
- **Wide glyphs in the terminal take their real two-column pitch.** A CJK or emoji
  character no longer draws at a one-cell pitch and overlaps its neighbour, so every
  following column stays lined up with the cursor and selection. The grid supplies a
  width per character (a combining mark takes none) instead of the font's advances (#140).

## [0.2.19] - 2026-11-15

### Added

- **Orbit reopens the window where you left it.** The main window's frame, whether it
  was maximized, and the display it was on are saved to `~/.orbit-pi/window.json` (half a
  second after a move or resize stops, and on close and quit) and restored on the next
  launch. A frame that no longer fits is shrunk and moved onto its display, and a display
  that is no longer attached falls back to the first-launch layout, so a window can never
  open off-screen (#131).
- **Manage models… from the model picker.** The picker's footer links to Settings →
  Providers & models, searched to the provider in view (the scoped provider, or the active
  model's).
  ⌘, (Ctrl+, on Windows and Linux) does the same while the picker has focus. The model
  picker's footer drops its arrow and Enter hints to make room; the thinking picker keeps
  them (#131).
- **Steering delivery mode in Settings → Agent.** Steering messages get the same
  `One at a time` / `All` choice that follow-ups already had, read back from pi's session
  state (#131).
- **Both send lanes in the composer.** While a run is in flight and something is typed, a
  quiet second button beside Send delivers it in the other lane — steer when Enter queues,
  queue when Enter steers — so a one-off choice no longer means flipping the preference
  (#131).
- **Tray and menu-bar icon, and a choice for the close button.** Settings → General →
  Closing the window: *Hide to tray* (Windows) or *Keep running* (macOS) keeps Orbit and
  any running task alive behind a notification-area / menu-bar icon with Show Orbit, New
  task and Quit; *Quit* quits through the same path as ⌘Q / Ctrl+Q, so it still waits for
  a fork in flight. macOS keeps running by default, as it always has; Windows and Linux
  keep quitting by default until you opt in. On Linux the icon is a StatusNotifierItem
  (KDE, Xfce, most panels, GNOME with the AppIndicator extension); where no host is
  running, closing the window still quits, so it can never vanish with no way back (#131).
- **Providers and models on one page.** Settings → Providers & models replaces the
  separate Models page: the active model leads, connected providers list their models
  directly under their cards, and providers that are not set up yet follow in their own
  section. One search narrows both, and the favorites filter and *Update model list* moved
  onto its toolbar (#131).
- **Choose which models the picker shows.** Each model card in Settings → Providers & models has an eye to
  hide or show it in the composer's model picker, and each provider has Hide all / Show
  all. Hiding never deletes: the provider's catalog is untouched, models it adds later
  still appear, and the session's own model stays pickable (#131).
- **Fetch models for a custom provider.** The provider editor's **Fetch models** asks the
  base URL's `/models` endpoint (OpenAI-compatible servers such as OpenRouter, Ollama, LM
  Studio and vLLM, plus Mistral and Anthropic) and adds every id not listed yet (#131).
- **Archive a whole workspace.** The workspace header menu gains *Archive all sessions*;
  archived sessions stay on disk and come back from the sidebar's archived view (#131).
- **Every language covers the whole app.** About 930 strings per locale still fell back
  to English — Git panel, MCP, subagents, settings, Explorer, wallpaper, updates,
  exports — in Simplified Chinese, Japanese, Korean, Spanish, French, German, Brazilian
  Portuguese, Russian and Italian. They are translated now; what remains English is
  deliberate (product names, Git terms some languages keep, example ids, and text the
  model itself reads). CI now fails when a locale gains an untranslated string outside
  `scripts/i18n_untranslated_baseline.json` (#131).

### Fixed

- **Line breaks in translated worktree messages.** `gen_locales.py` doubled the backslash
  in `\n`, so eight worktree errors showed a literal `\n\n` in every non-English locale.
  Regenerating the locales also no longer reverts translations that were edited straight
  into a locale file; those now live in `scripts/i18n_glossary_preserved.py`.
- **Orbit no longer fails to start when `pi-subagents` is installed.** pi 1.5 treats two
  extensions registering one tool name as a fatal load error, so the bundled `subagent`
  extension collided with `pi-subagents` and every session exited before it began. The
  tool is now registered when the session starts rather than while the extension loads,
  which is also the first moment `getAllTools()` can answer: the ownership check that was
  meant to stand down had been reading a throwing stub as "no tools yet". Orbit now stands
  down when another extension owns the name, and Settings → Subagents shows a banner naming the
  owner instead of a catalog that has no effect on the session (#122).

### Changed

- **Composer "+" menu picks several images or files at once.** "Attach images…"
  and "Attach files…" open a multi-select picker, and the chosen items land in
  the tray in the order the picker returned them. Decoding and compression run
  off the UI thread. The shared eight-item cap still applies; when a selection
  overflows it, the status line says how many were added and how many were
  turned away.
- **Quieter, shorter sidebar rows.** The open session is marked by the
  `surface_selected` fill alone: a custom accent no longer tints it, and the
  "current" badge, selected border and semibold title are gone, so the title
  never changes width when the selection moves. A grouped row stops repeating
  its workspace's branch down the list — the location strip now appears in flat
  lists and search, and under a project header only for a session in another
  folder. First-message previews are off by default (the sidebar view menu still
  turns them on), leaving a grouped row one 28px line: title, status and age.

## [0.2.18] - 2026-11-12

### Added

- **Settings → Permissions: per-command and per-agent rules.** The access mode
  could only say "ask before commands" or "do not". The page is one row per tool
  family (commands, edits, reads, web fetches, MCP tools, delegation, other) with
  `Inherit · Allow · Ask · Deny`, the value it resolves to shown beside it, and
  pattern rules beneath (`rm *` → Deny) added inline. The scope row switches the
  whole page between every session, main sessions only, and one subagent, so
  "`bash` may run, but `rm` asks" and "the worker may run commands" need no mode
  change. Most specific scope wins, and inside it deny > ask > allow. Full
  access is answered before the rules — the mode keeps its promise that nothing
  asks and nothing is refused, and the page says so instead of offering rows
  that cannot take effect. Rules live in `~/.orbit-pi/access-rules.json`, which
  the guard extension reads on every tool call, so they apply to running
  sessions.
  Two deliberate limits: an `allow` never covers a chained or redirected command
  (`ls; rm -rf x` still asks), and an `ask` rule is a refusal inside a subagent,
  which has no approval window. Web fetches are now their own family so they can
  be gated; their default behaviour is unchanged. See
  `crates/orbit-pi/docs/permissions.md`.
- **Glass panes over the wallpaper.** Panes that sit over a wallpaper now
  frost what is behind them instead of showing the picture sharp through a
  tint: the sidebar, the composer, and every floating layer (menus, pickers,
  popovers, modals, notifications). The image stays sharp in the content area
  beside them, which is what separates glass from a uniformly blurred
  backdrop. Text contrast is unchanged — the fill keeps the alpha the contrast
  pass computed, and only the pixels *under* it are blurred. Settings →
  Appearance → Glass panes turns it off. macOS only: the frost is a Metal pass
  over the frame (`backdrop-filter: blur()` has no GPUI equivalent, so gpui
  0.2.2 is vendored under `vendor/gpui` with the marker, the shader and the
  blur pass added — see `vendor/gpui/README.orbit.md`); elsewhere the panes
  keep the plain translucent fill they had before.

### Changed

- **The sidebar and the chat are one plane again.** The chat column no longer
  rounds its left corners, and the hairline between it and the sidebar runs the
  full height of the window instead of stopping an 8px radius short at each end.
  The curved notch the two corners left against the sidebar was the most
  noticeable detail in light mode.
- **Typing no longer rebuilds the window.** A composer edit used to repaint the
  app on every keystroke so the send control could move between two states; in a
  long session that paid for the sidebar, the header and every visible
  transcript row to redraw one text caret. The observer now compares the facts
  the app-level chrome actually reads — the send control, the attachment tray,
  the follow-up chips, the slash/`@` menu — and repaints only when one of them
  moved. The text itself, the caret and the box's grown height were always
  drawn by the composer and still are.

### Removed

- **Settings → Privacy no longer lists or deletes temporary files.** The
  **Temporary files** and **AI scratch files** sections are gone, along with
  the scanning and cleanup code behind them (`temp_files.rs`, `temp_ui.rs`).
  Session retention remains. Bug-report screenshots, update staging, and login
  helpers are still written to `<system temp>/orbit-pi/`; Orbit simply stops
  offering to enumerate and remove them, so clean that folder up yourself if
  you want it emptied.

## [0.2.17] - 2026-11-08

### Changed

- **A delegation card opens one child's log, not all of them.** Clicking a
  child row shows that child's steps and report and leaves the row selected;
  clicking it again closes the card, and picking another child swaps the body.
  A parallel run with three finished children used to expand into three
  unlabelled logs in a row — the steps were there, but not whose they were.
  A one-child delegation is unchanged: opening the card is opening its log.
- **`@` mentions no longer freeze the window while you type.** Each keystroke
  used to spawn `fd` on the UI thread and wait for it — 300–700 ms per
  character on a large project, and worst of all when nothing matched, because
  `--max-results` could only short-circuit when hits existed. The composer now
  ranks an in-memory index of the workspace instead: the same queries cost
  milliseconds and spawn nothing. The index is built off the UI thread, rebuilt
  at most once every 2 s after a change, and immediately after your own file
  operations, so a created, renamed, or deleted file is in the next `@`.
  Dotfiles stay searchable as they were (`.env`, `.github/**`) while
  version-control and OS metadata (`.git`, `.svn`, `.hg`, `.jj`, `.DS_Store`,
  `Thumbs.db`) is never indexed, and a `.gitignore` in a *parent* directory is
  no longer consulted — matching `fd`, and the same walk the Explorer uses.
- **A `/` command opens after any whitespace, not only at the start of the
  message.** Typing `run /rev` opens the command menu with the caret still in
  that word, so a skill or extension command can be reached mid-sentence the
  same way it can at column 0. A slash glued to the previous word (`http://`,
  `please/review`) stays plain text, and so does a path like `/usr/local`: the
  menu opens on it but no command matches, and the word is never coloured as
  one.

## [0.2.16] - 2026-11-05

### Added

- **Subagents.** The agent can delegate a scoped task to a child `pi` process
  with its own context window and get back a short report plus its cost, instead
  of spending the session's context on the reading. One tool, three shapes —
  single, chain with `{previous}`, and parallel — bounded at 8 tasks per run,
  4 concurrent, 24 per session and depth 1. Agents are markdown files in
  `~/.pi/agent/agents/` (the path `pi-subagents` reads); `scout`, `worker` and
  `reviewer` ship built in. A child inherits the session's access and workflow
  modes and is clamped to the ceiling its declared tools imply, so a read-only
  agent cannot edit even under Full access. A delegation the session mode cannot
  finish — a command-running agent under Supervised or Plan — is refused before
  it spends anything. Abort kills the whole child process group; a child's cost
  lands in the session usage meter. The transcript shows a delegation card —
  each child's agent, capability, current step, tool count, elapsed time and
  model, the run's cost, and the mode that blocked or refused a call; expanding
  it reveals the child's real steps and the report it returned. The parent's
  working line names what the child is doing, and the turn header counts
  delegations. **Settings → Subagents** manages the library in-app: create,
  duplicate, rename, disable, delete (with Undo) and reset an agent to its
  shipped definition; edit its capability tier, workflow mode, model, thinking
  level, description and prompt; and see the exact description block the model
  will read. Saves touch only the five keys Orbit owns and refuse to clobber a
  file edited elsewhere. Inspectable child transcripts are not built yet.

- **Export a response or visible transcript as Markdown.** Each settled response's
  footer offers a save action; the command palette can export the visible transcript.
  Both open a native save dialog and exclude thinking, tool output, and image data.
  The visible transcript can omit earlier history after compaction. Existing files
  are never overwritten. GUI verification remains.

- **"Work in" is one control, and it answers for every kind of place.** The
  status bar's chip now reads `Local` at the repository's main checkout (it
  used to appear only inside a worktree, so the composer could never enter one)
  and offers the repository's worktrees, a new worktree, an existing worktree
  folder on disk, a remote branch, a clone, recent folders, and any other folder.
  The new-task page's folder field uses the same vocabulary, so a task can
  start in a worktree without visiting the Worktrees page first. Choosing a different location
  starts a task there; a running session is parked, and an idle session's
  transcript remains on disk.

- **Clone a repository from the working-folder picker.** Enter a URL and folder
  name, choose a destination, and follow Git's progress or cancel. A successful
  clone opens as a working folder and joins the sidebar; a failed or cancelled
  clone is not opened, and Git may leave a partial directory. The new-task picker
  offers cloning even outside a repository. The Windows job-object cancellation
  path now runs on the Windows CI leg (`repository_clone::` tests); manual GUI
  verification remains.

- **Remote branches become local branches that track them.** Choosing a
  remote-tracking ref used to run `git worktree add <path> origin/feat`, which
  leaves a **detached** worktree with no branch to push. It now creates
  `feat` (or `feat-2` when that name is taken) with `--track`, and the create
  dialog says which branch it will create before you press Create.

- **A worktree folder Git stopped listing can be reconnected.** A directory
  that was moved by hand shows up as prunable, and the picker's
  "Existing worktree…" row (and the Worktrees page's Advanced menu) can reconnect
  the chosen directory with `git worktree repair` before opening it. A worktree
  whose administrative metadata is gone is reported instead of opened.

- **Rewind a conversation to an earlier message.** A user message's footer now
  offers a rewind beside its copy action, and the palette's *Rewind to a
  Message…* lists the same messages for the keyboard. pi forks: the branch up
  to just before the chosen message is written to a **new** session file, so
  the conversation you leave stays in the sidebar and nothing is destroyed. The
  forked-from prompt is handed back to the composer to edit and resend. A run
  in flight has to stop first, and an extension that cancels the fork leaves
  everything untouched.

- **Discover session shortcuts by holding ⌘ or Ctrl.** The first nine visible
  sessions show their numbered keycaps while the modifier is held; the hints
  follow the current sidebar order and disappear on release.

- **The command palette advertises both of its shortcuts.** `⌘P` and `⌘K` have
  always both opened it, but only `⌘P` was shown on Settings → Shortcuts.
  Both chords now appear, so a user who learned `⌘K` is not told otherwise.

### Changed

- **A check reads the feed; downloading is your decision.** The updater used to
  fetch and stage the whole artifact during the background check, so the footer
  pill appeared only after 20–40 MB had already moved and clicking it went
  straight to the swap. A check now stops after the feed and names the target
  version beside the running one, with a **Changelog** link to the release page
  and the notes already rendered, before any bytes are requested. **Download**
  fetches, verifies, and stages with a real progress bar and a Cancel; when it
  finishes the same card offers **Restart & Update**, which is what hands the
  swap to the installer helper. The pill's glyph follows suit: a download arrow
  while the release is only offered, a restart arrow once it is on disk. Two
  consequences worth knowing: the modal's copy no longer promises that
  downloading installs, and a silent launch check costs one small request
  instead of a download.

- **The `+N −N` chip opens the Git page's Changes tab.** The dirty-tree
  counts now reveal the files they count instead of opening the Review dock;
  the plain side-panel glyph still toggles Review.

- **Labelled buttons no longer repeat their label on hover.** New Task, the
  Git page tabs, a widened access-mode chip, and sidebar session titles drop
  the tooltip that restated text already on screen; icon-only controls and
  truncated previews keep theirs. Session rows now show their shortcut hint
  on hover instead of a title tooltip.

- **Composer delivery hints use keycaps.** Queue and Steer show the same
  platform-aware keyboard styling as the sidebar while keeping the shortcuts
  matched to the selected Enter behavior.

- **Settings is easier to scan and use.** Navigation rows keep their full
  height in short windows, page and section headings have clearer hierarchy,
  descriptions use the regular UI text size, and select controls match the
  app's 32px buttons. The selected navigation row uses a neutral surface.

- **Session search shows the command palette shortcut.** Compact keycaps in
  the empty sidebar input show ⌘P / Ctrl+P for the separate command palette.
  The footer Search icon is removed. ⌘⇧F / Ctrl+Shift+F now focuses session
  search, while ⌘P / ⌘K (Ctrl+P / Ctrl+K) open the command palette.

- **Workspace icons fill more of their existing sidebar frames.** Project logos
  and marks have less inner padding, and their frames use a neutral surface
  instead of an accent tint. The frame size is unchanged.

- **Windows installs and updates pi with pi's own installer.** Orbit used to
  tell Windows users to run `npm install -g` itself and refused to migrate an
  npm install there, on the belief that pi shipped no installer for the
  platform. pi documents one — `powershell -c "irm https://pi.dev/install.ps1 | iex"`
  — and it lays down the same pinned, self-updating managed install under
  `%USERPROFILE%\.pi\agent\install`, so install, migration, and `pi update`
  now work on Windows the way they already did on macOS and Linux. The setup
  page's copyable hint follows the platform, and an npm global install on
  Windows (whose `pi.cmd` shim sits beside `node_modules` rather than inside
  it) is now recognised as an npm install instead of being reported as a
  package manager Orbit will not touch.

- **Typing in the command palette no longer rebuilds its rows.** Session,
  command, settings, and MCP rows are built once when the palette opens and a
  keystroke only filters and ranks them, instead of re-deriving every label
  (and re-reading every session title) on each key. A row whose *name* matches
  now outranks one that only matched a search keyword.

### Fixed

- **Settings dropdowns paint their options again.** Every select in Settings —
  theme, font, model, retention, language — opened an empty panel: the option
  list is `w_full` inside a popup that carried only a `min_w`, and a
  percentage width inside an auto-width parent resolves to 0, so each row laid
  out 8px *narrower than nothing*. The popup now sets an explicit width, and a
  layout test checks the list and its rows keep a real width.

- **Tests no longer add temporary projects to the real sidebar.** Workspace
  preferences and the last-workspace file use isolated, self-cleaning test
  directories instead of the user's Orbit files.

- **Sidebar session search now filters the visible list.** Search results and
  keyboard navigation use the same sessions, respect the archive filter, and
  show each result's workspace. Clearing the search restores the list.

- **The active sidebar session uses the hover surface.** Its background stays
  neutral even with a custom accent; the border still marks the open session.
  Other list rows use their palette's selected surface, while text selection
  keeps its own highlight.

- **Runtime settings show where pi actually is.** The page listed the pi
  *package root* — a `node_modules/@earendil-works/pi-coding-agent` path deep
  enough to take the whole row — and because a control never shrinks it
  squeezed "Package root" into one letter per line. The deep path now rides
  the RPC-patches row as a home-shortened, truncating detail line, and the
  binary row keeps naming the launcher Orbit spawns, which is what `which pi`
  prints.

- **Dismissing the command palette returns focus to whatever opened it.** It
  always went to the composer, so opening it from the sessions sidebar and
  pressing Escape dropped the keyboard into the wrong surface; running a row
  left the window with no focused element at all, so typing did nothing until
  the next click.

- **`⌘P` / `⌘K` are no longer swallowed just after a menu closes.** The 200 ms
  guard that stops a dismissing click from immediately re-opening a popover was
  applied to the keyboard path too, so a shortcut pressed right after closing a
  chip or a row menu silently did nothing.

## [0.2.15] - 2026-11-01

### Added

- **File count and previous / next arrows in the Review toolbar.** The diff
  toolbar names how many files the snapshot changes when there is room and
  walks between them with previous / next arrows around the reader's
  place (`12/187`). The arrows and the `n` / `p` shortcuts share one step,
  measured from the file at the top of the diff so scrolling to another file
  and pressing the arrow continues from there; both go inert at the first and
  last file. The count and navigator fold away as space gets tight rather than
  overflowing the row.

### Fixed

- **"Open in" Rex opens the folder when Rex has to start.** A folder handed to
  Rex while the app was still launching was dropped: Rex came up on its About
  window and never opened the workspace, because a launch-time open request
  arrives before its client has connected to its server. Opening in Rex is now
  two steps when the app isn't running — start it, then hand it the folder a
  moment later — which opens the workspace. Rex's About window still appears;
  only the lost folder is worked around, and a Rex that is already running is
  handed the folder in one step as before.

- **One usage card per Ollama account.** A setup with both `ollama` and
  `ollama-cloud` configured showed the same account twice — two cards carrying
  identical windows and balances, and a header count to match — because the
  popover rendered one card per provider id while both ids resolve one
  ollama.com account (the adapter borrows a key stored under either). The
  popover now collapses ids that share an account into a single card, named
  after the id the active model uses, preferring the report that carries
  numbers over an errored sibling and keeping a lone failure visible rather
  than folding it away. The eye toggle stores the shared id, so hiding the card
  survives a model switch between the two, and an entry saved under either id
  before this change still hides it.

- **Ollama's monthly credit window explains its missing countdown.** The note
  naming the plan-anniversary reset was dropped whenever the session and weekly
  windows carried their own reset times — which is always — because a note only
  rendered when *every* window lacked one. It now renders when any window does.

- **An expired Ollama session reads as expired, not as "fetch failed".**
  Redirects were refused at the fetch layer, so the settings-page reader never
  saw the sign-in redirect it was written to translate. An expired cookie now
  reports "Ollama session expired — sign in again", while the usage endpoints
  still refuse to follow a redirect at all.

- **A nested `/api/balance` payload keeps its meters.** The balances parser already
  unwrapped the payload's optional `balance` container, but the window parser
  read `included` from the top level — so a nested payload showed its purchased
  credit and silently dropped the session and weekly meters. Both parsers now
  unwrap the same container, and a bucket's reset is read under any of the names
  it has shipped with.

- **A dead Ollama endpoint is reported, not disguised.** When one endpoint
  failed and the other carried nothing to show — `/api/balance` dying while
  `/api/usage` has no meters is the normal shape for a current account — the
  card read "reported no usage or balance for this account", describing a
  healthy account that uses nothing rather than the endpoint that did not
  answer. It now reports the HTTP failure, from either side.

- **The skill enable/disable switch is clickable again on Windows.** Settings →
  Skills puts the switch at the right edge of the detail header, directly under
  the caption buttons the app paints over the window's top-right corner, so the
  maximize button covered it and swallowed the click. The header now reserves
  the caption strip, the same inset the Files and Explorer headers already use.

- **Enter and Backspace during an IME composition no longer act on the
  draft.** Sogou Pinyin, and Chinese IMEs generally, keep the composition
  session — and its candidate window — open after Backspace has emptied the
  pinyin buffer, and report that as an empty preedit. Orbit read the empty
  preedit as "not composing", so Enter submitted the draft and Backspace
  deleted committed text behind it. The composer now tracks the session
  separately from the preedit span and holds its own shortcuts while one is
  open; on Windows it also reports the empty span as marked, so the platform
  hands the keystroke to the IME instead of dispatching `Submit` /
  `Backspace`, and a session the IME abandoned on focus loss is closed on the
  next frame instead of claiming keys. Regression tests cover the Sogou
  sequence (Backspace down to an empty pinyin buffer, then Enter / Backspace /
  arrows), the Windows marked-span gate, and session teardown on focus and
  window loss.

## [0.2.14] - 2026-10-30

### Added

- **Edit a queued message, or send it now.** The queue bar above the composer
  gives every pending steer or follow-up its own **Edit** and **Send now**
  controls. Edit opens the row in place with Save and Cancel (⏎ saves,
  Esc cancels) and rewrites just that message; Send now moves a queued message
  into the steering lane at the front, so pi delivers it before the next model
  call instead of behind the rest of the queue. pi has no per-message queue
  command, so Orbit rebuilds the queue — clear, then re-queue the survivors in
  their original lanes and order — and keeps the bar from blinking through the
  empty state while it does.

- **Every installed terminal is in the "open in" menu.** Rex, Ghostty, iTerm2, Kitty, Alacritty, WezTerm, Tabby, Hyper, Rio, Wave and Warp appear next to Terminal once their app is installed, and tmux appears when the binary is. tmux has no window of its own, so it opens a session at the workspace in the system terminal; the list scrolls instead of growing past the window.

- **GitHub CLI release check:** Settings → Git Accounts can check the latest stable `gh` version on request and show manual update commands. Normal status refresh stays local; Orbit does not update the CLI or use credentials for the check.

- **Bug-report draft model picker:** choose a model for Generate draft independently of the active session and Git commit-message model. The choice is remembered, with the active session model as the default.

- **Git Accounts diagnostics** in Settings: inspect local author identity, GitHub CLI version, SSH agent fingerprints, conventional public-key file presence, generic and host-scoped credential-helper configuration, sanitized remote hosts and credential environment-variable presence without managing credentials or testing remote access. GitHub CLI authentication is shown as unknown because its status command may contact hosts and refresh credentials.

### Fixed

- Pasting files copied in macOS Finder resolves file-reference URLs to real paths and attaches image contents rather than the generic file icon. Windows file lists and Linux file URI lists exposed as text also use the attachment tray; other files use the existing file references.

- **Global Refresh is a full resync.** ⌘R, File → Refresh, and the command
  palette entry now re-read the session list, branch lines, installed plugins,
  workspace skills, custom providers, auth files, and MCP configuration even
  when their Settings page is closed, and re-ask the running agent for its
  state, model/thinking/command catalog, context stats, auth, and quota. The
  refresh confirms with a toast, so a manual refresh never looks like a no-op.

- **Follow-up suggestion chips stay inside the composer.** A long model
  suggestion wraps to two lines inside its chip and the chip is capped at the
  composer column's width, so the row can no longer push past the window edge
  on a narrow window. Clicking a clamped chip still inserts the full text.

- **Agent errors are reported for the turn, not for every attempt.** A retry
  that recovers retires the failed attempt's card, banner, and "Agent error"
  notification, so a turn that finished is announced as finished. A stop the
  user asked for reads as the quiet "Stopped" marker even when it landed
  while a provider request was in flight (pi records it as an error carrying
  the SDK's abort text) or cancelled a retry. A turn that really ended on a
  provider error still reports it.
- **Review and Git changes refresh** after a file change or manual refresh arrives while a diff is still loading; diff failures are shown instead of leaving stale results silently visible.
- **Bug-report screenshots** fall back to GitHub's web form when the CLI cannot upload them without repository write access or another preflight requirement. Orbit opens the saved screenshot folder for dragging into the form, preserves colliding filenames, and names compressed images for their actual format.

### Changed

- **The Usage Summary board reads any day under the pointer.** Each headline
  metric (requests, tokens, cache hit rate, average response) now carries a
  40px sparkline of the selected range, following the bklit KPI-sparkline
  recipe: the line draws itself in on first paint and a range change morphs the
  old shape into the new one. Pointing at any card's line swaps the whole row's
  figure, date, and change to that bucket (its change measured against the
  bucket before it) and highlights that point immediately — hovering never
  replays a transition. Leaving the board snaps every card back to the range.
  Reduced motion paints the final frame. A bucket with no measurement for a card
  reads as a gap and a dash, never a zero.

- **Usage breakdown reads as one legend.** Models, providers, workspaces, and
  tools share a single row design (bklit's legend with progress bars): a brand
  tile — the model's or provider's mark from the model picker, the workspace
  folder, or the tool's glyph from the transcript — then the name and its scope,
  the figure with its share, and a track whose fill is the share. The same mark
  leads each row in the table below. Clicking a row still scopes the page, and a
  row in scope reads as selected. Hover washes the row at once, with no
  transition.

### Fixed

- **Stop and Esc now interrupt a stuck pi.** They sent `abort` and then waited
  for `agent_settled`, which a pi whose event loop is wedged never sends — one
  bash call that dumps several GB (`rg -r '' -n ""` over a large repo) was enough
  to freeze it, and both controls went silent while the turn stayed “Working
  for…” for minutes. A stop is now timestamped: the composer says it is waiting
  on pi, pressing again after a moment acts at once, and if pi has not answered
  within 10 s Orbit closes the turn itself, takes the process down and starts a
  fresh one on the same session. The process kill is a tree kill on Windows, so
  the wedged tool child goes with it instead of outliving the restart. Queue text
  that was waiting behind the stop is handed back to the composer.

## [0.2.13] - 2026-10-27

### Added

- **Optional sidebar message previews.** The sidebar view menu's **Show first
  message** toggle hides or restores the first-message line in grouped and flat
  session lists. Enabled by default, remembered across restarts, and leaves titles,
  metadata, session content, and actions intact.

- **Branch pickers show when each branch last moved.** Every row carries the
  age of its tip commit (`3d ago`) in a right-aligned column: in the status-bar
  branch popover the dates read as a timeline, newest work first, with the
  current branch marked by an accent glyph and a check in a slot every row
  reserves so the dates never shift; the header counts what the filter found.
  The Git panel's branch menu dates its Recent, Local, and Remote sections the
  same way (the age is the row's last element, so all dates share one right
  edge next to the `Current` marker or the hover delete), and remote refs now
  carry their tip date in `list_refs`. Filtering still matches branch names.

- **Session execution status in the sidebar.** Open and background sessions show
  Running, Waiting for approval/input, Retrying, Compacting, Completed, Failed,
  Stopped, Disconnected, or Idle from real pi events. Completed/Failed/Stopped
  remain after opening a result and after warm-process reaping for this launch.
  Historical and external sessions have no guessed status until observed.
  Only `agent_settled` finishes a run, and its abort flag distinguishes a stopped
  run even when no assistant message was produced. Native approvals, dialogs,
  and questionnaire answers now stay with their session when switching away;
  requests arriving in the background wait for the user instead of being cancelled.
- **Mermaid flowcharts render in the transcript.** A `flowchart`/`graph` fence
  (TD/TB, BT, LR, RL) now paints as a diagram — nodes, shapes, orthogonal edges,
  arrowheads, and edge labels — drawn with GPUI paths and text, so there is no
  webview, no JavaScript, and no network involved. The fence is still a code
  block underneath: the copy button and a **Show source** disclosure keep the
  exact Mermaid source selectable, and anything outside the supported subset
  (sequence/gantt/pie/class diagrams) keeps the existing source block rather
  than drawing a partial graph. `subgraph … end` groups render as titled frames,
  and an edge that skips a rank routes around the boxes instead of through them.
  Layout is cached per fence and interface font, so a streaming run re-renders
  the same diagram for free.
- **A session that finishes while you are not looking stays marked.** Its sidebar
  row carries an unread mark — a green eye and "Needs review" for a finished turn,
  a red cross with a small corner dot for one that failed — until the session is
  actually shown in a foreground window. That mark then gives way to the read
  glyph, while a separate execution state distinguishes Completed from Failed and
  remains after the mark is spent. The mark is what is left after the toast has
  gone: with several sessions open, or after stepping away, the finished one is
  findable instead of merely somewhere in the list. It is independent of the
  running, pinned, and archived glyphs (it never replaces one) and a marked row
  stays visible under a collapsed project group, the same way a running row does.
  Opening the session spends the mark; selecting it while Orbit is behind another
  app does not, because that is not reading it, and neither is settling behind a
  full-page surface (Files, Git, Usage, Worktrees, or a full-width Review).
  Marks live in memory only — what happened while Orbit was closed is not claimed,
  since there is no way to know whether it was already read elsewhere. The labels
  distinguish a finished turn from a failed one without relying on colour.
- **Onest interface font** — available in Settings → Appearance → Interface font,
  bundled in Regular, Medium, SemiBold, and Bold weights for offline use.
- Git page → commit bar → **model chip**: the **Generate** button now chooses its
  own model instead of always borrowing the chat session's. Commit messages are a
  short, cheap task, so a small local model is often enough, and a model with a
  different context window changes how much of a large change set fits the prompt.
  The chip opens a searchable catalog with each provider's mark, an
  **Active session model** row that restores the default, and ↑/↓/Enter/Escape
  navigation; the pick persists to `~/.orbit-pi/commit-model.json`, and the prompt
  budget is sized from the picked model's own window. Issue and PR draft generation
  keeps using the session model.
- **Attachments in the composer** — images, non-image files, and collapsed long
  pastes now share one tray above the input. Each item is a chip leading with the
  same 20px slot — the image thumbnail, or the file type's devicons mark — then its
  name, its size or line count, and its own remove button; the text sits in one
  `line_height(1.)` box and the chip's insets are symmetric, so the mark and the label
  share one centreline with balanced edges. The file mark is drawn bare, with no tinted
  square behind it — a rounded square behind a glyph whose art is itself a box read as
  an avatar monogram, not a file icon — and it is centred by its *ink* rather than its
  line box, because glyph artwork is not centred inside its font cell. The type's short
  label (`MD`, `PDF`, `ZIP`, …) on a tinted square fills the slot when no Nerd Font or
  no glyph is available; two or more chips add a count header with **Clear**, and all three kinds
  share one eight-item budget that the send button and the paste/drop paths read
  through a single predicate. Attaching a file queues it as a chip instead of
  splicing its path into the middle of the message; on send it folds into the
  prompt as an `@path` mention (relative to the workspace, backticked when the
  path has whitespace) so the agent opens it with its own tools. The sent user
  message reads that block back out of its own text and renders it as the same
  file chips under the bubble — the path stays selectable — so an attachment no
  longer appears as a raw `@/Users/…` line, and a reloaded session shows the same
  chips because the split is derived from the message, not from local state.
  Re-attaching the same path is a no-op, and a file-only message is sendable.
- Appearance → **Design system**, an opt-in switch between the existing **Classic**
  design and the new **Operational** (Restrained Operational Minimalism) system.
  Operational adds dark and light near-black/neutral palettes with a lime
  interaction accent, a separate olive selected-row wash, tighter radii, 11px
  captions, and flatter elevation. The choice is persisted alongside the existing
  Light/Dark/System preference; switching back restores the previous Classic
  palette, and every Classic palette and geometry stays unchanged.
- Settings → Agent → **Agent call details** chooses whether a tool call or
  activity group you have not opened yourself follows the run (the previous
  behavior: open while streaming, closed once the turn settles), stays always
  expanded, or stays always collapsed. Opening or closing one call yourself
  still wins over the setting, which only moves the default. Persisted to
  `~/.orbit-pi/agent-calls.json`.

### Changed

- **Refined session navigation.** Tinted project marks, tonal count and status
  badges, clearer title/preview hierarchy, and distinct selection and keyboard
  focus outlines. Provider marks overlap less, and truncated titles, prompts,
  and branches have tooltips. Existing session information and actions remain.
- **Session rows carry three lines.** Title, the first message, and a metadata
  strip — age, workspace, and branch with `↑n`/`↓n` against its upstream (green
  ahead, amber behind). The message line always shows the prompt, because the
  title is capped and the row is narrower still: a title that happens to be the
  same sentence is cut short, not redundant, so the message is the only place
  the ask reads end to end. Branches are fetched off-thread per project (`git
  branch` + `rev-list`, one pair at launch and whenever the project list or the
  current checkout changes), so painting a row never shells out. A project that
  is not a git repository has no branch item rather than an empty slot. The
  workspace item appears in the flat list only — under a project header it would
  repeat the header one line up.
- **Session row actions moved to the right-click context menu.** The
  hover-revealed `…` is gone, so the title owns the full line width; pin,
  archive, copy path, reveal, clone, and delete are all on the context menu
  (the palette still carries its own commands). The keyboard and palette delete
  paths anchor the same confirmation to the row instead of to a button that no
  longer exists.
- Sidebar project names no longer look like the session titles under them. A
  group header takes the palette's accent hue at body-ink brightness — a quiet
  warm cast by default — or, when the project has a mark tint, that hue on its
  name and not only on its icon. Session titles stay a step below at secondary
  ink. The three sidebar levels now read as section label, project, session
  instead of one wall of identically colored rows.
- Standardized corner radii by component role: buttons and header chips use the
  button token, text fields and composers use the input token, cards/panels/
  popovers/dialogs use `panel::RADIUS`, list rows use `list_item::RADIUS`, and
  badges/checkboxes use the small radius. Fixed the outliers (header controls,
  the top-bar quota/usage chip, terminal and file-viewer tabs, the Worktrees
  dismiss button, MCP row action, star-project banner, several alert/preview
  cards, and the Git commit composer).
  Operational now gives every standard field and composer one input radius;
  Classic keeps its legacy composer silhouette.

- The Usage page is now one continuous page instead of a Simple | Details
  switch: the Daily-activity calendar leads, then the Summary board, Usage
  over time, Breakdown, and Records read top to bottom in one scroll, with
  each section's own selector (chart metric, breakdown dimension, records
  table) inside its card. Summary figures are larger, and the metric the
  trend is currently plotting is picked out in the accent color on the board,
  so the click-through from a figure to its chart reads before it is made.

### Removed

- Usage: the trend's duplicate Buckets data table, the Signals notes card,
  and the Token health panels are gone. Their headline figures live on: cache
  hit rate is a Summary metric, every metric still plots on the trend chart,
  and the day/week/month table remains under Records. The JSON export no
  longer carries the `insights` block.

### Fixed

- **Auto session titles now actually land.** The bundled title extension decided
a session was worth naming only when its branch held *exactly one* user prompt at
settle time, and it treated a settle with nothing to describe as final:

  - sending a steering message or follow-up during the first run added a second
    prompt, so the settle looked like a resumed session and no title was asked for;
  - a session Orbit reopened from disk (its warm-process pool makes this the
    normal case) was permanently ineligible, however untitled it was;
  - one transient provider error was enough to lose the title for good — the
    "retry on the next settle" path needed another settle with one prompt,
    which a new prompt makes impossible, so the retry was dead code;
  - a settle that arrived before the assistant had produced any text wrote the
    session off for the life of the process.

  The rule is now the one a reader expects: **any session without a name gets a
title**, from its first exchange, on the first settle that has something to
describe. An existing name is never replaced (that is the user's rename or an
earlier title) except by the details popover's explicit **Generate title**. A
settle with no reply yet leaves the session open for the next turn, and a
transient failure is retried up to three times instead of once. A completed ask
that produced no text is still final, so a model that keeps answering nothing is
not asked on every turn.
- The status bar no longer prints extension color codes as literal text. An
  entry set with `ctx.ui.setStatus()` that styled itself via `ctx.ui.theme.fg()`
  used to render the raw SGR escape (e.g. `[38;5;71m●[39m`), because the status
  line drew its string verbatim. It now passes through the same ANSI parser the
  extension-widget path already uses, so the codes are stripped and the entry
  reads `● ADHD ON` as intended.
- The Git page's **Generate** button now writes a commit message that describes
  what the commit actually records, including long files. Four things were
  wrong: the prompt always read the index (`git diff --cached`) even when
  *Include unstaged changes* was on, so most of a worktree commit arrived
  labelled "context only, not part of this commit" and untracked files arrived
  as a bare path with no diff at all; ORIGINAL CODE was the first 48 KiB of the
  HEAD file with no line numbers, so a change deep inside a long file had no
  surrounding code to read; diffs were capped per file but never in total, so
  a set of long files overflowed the model's context window and silently
  became the filename-based fallback; and every body line was rewritten into a
  `- ` bullet, mangling the headings and footers a long change produces. The
  prompt now picks its diff source from what the commit will record
  (synthesizing untracked files' diffs), windows a line-numbered ORIGINAL CODE
  excerpt around each hunk, sizes the whole prompt from the selected model's
  context window so a commit of a hundred-plus files still gets a diff for every
  one of them (and keeps each long diff's first and last hunks), and passes the
  model's fenced message through verbatim. Regenerating now tells the model
  about the previous attempt, so it stops repeating the message you rejected,
  and a fallback is reported in the failure banner instead of being passed off
  as generated. Set `ORBIT_COMMIT_PROMPT_DUMP=/tmp/prompt.txt` to write the exact
  prompt the model sees.

- The Usage page's daily-activity calendar no longer lets its hover readout
  paint over the shade key. Hovering a day in the middle of the grid pushed
  the readout past the bottom of the calendar, where it covered the "Less /
  More" legend and the text under it. The readout now sits beside the day
  under the pointer and is clamped inside the calendar, so the hovered day
  stays visible and the readout never reaches the legend or the section
  below.

- A failed command or shortcut no longer leaves Orbit stuck on a banner that
  never clears and a session that answers nothing. A failed write to pi's stdin
  used to kill the writer thread for good while the process kept running, so
  the app still reported it alive and every later command and shortcut went
  into a pipe nobody read. The dead transport is now reported as an exit, so
  the reconnect action appears instead of a zombie session, and the app no
  longer leaves a response waiter behind for a command that never landed.
- An error banner now clears when the thing that raised it succeeds, decided by
  the raising command rather than by matching the banner's text. Failures whose
  wording never matched a command label — a failed write, or a banner raised
  under one language and read under another — used to stay up indefinitely.
- Stopping or restarting pi now drops that process's modal dialog, approval
  bar, queued messages, and retry counter with it; the dialog could never be
  answered, and its reply would have gone to the process that replaced it.
- Extension UI left over a dead pi process is dropped with it. A `custom`
  surface (what a command like `/review` opens) stayed on screen after the
  process that owned it went away, swallowing keystrokes into a client that
  could never answer it, and a compaction or title command in flight kept the
  composer on Stop because `is_compacting` stayed set.
- Commands pi runs itself no longer leave the app "working" forever. A slash
  command pi hands to an extension (`/review`) answers `prompt` with
  `disposition: "handled"` and starts no turn, but the composer had already
  armed the live "Working for…" row at the send, so the row and the Stop button
  stayed up with nothing that could ever clear them — cancelling the command's
  custom UI could not help, because there was no run to settle. Only the
  composer's own prompt is unwound this way: the title regenerator sends a
  `prompt` too, and regenerating a title mid-run used to close the live row and
  the checkpoint turn of the run that was still streaming.
- Cancelling a run no longer reports it as a failure. pi answers a stopped
  automatic retry with `auto_retry_end { success: false }`, which the app read as
  a failed retry and left on screen as a banner; the request itself now decides,
  so a stop is a stop and an exhausted retry still reports. That applies to the
  Retry strip's Cancel button as well as to Escape.
- A pi process that exits mid-session is respawned automatically, so the next
  command or shortcut lands on a live process instead of a toast saying nothing
  is running. The respawn is bounded (three consecutive tries, a budget a
  healthy run refills) and yields to a Start or Stop in the meantime, so a pi
  that is broken on startup shows the Reconnect banner instead of crash-looping.
  It also comes back on the session that was open: a fresh launch opens a new
  one, which would have left the transcript showing a conversation pi no longer
  had and the next prompt reaching a model with none of it. Restart and
  Reconnect do the same.
- A pi process that dies at launch now says why. The banner carries pi's own
  last stderr line (redacted, clipped to one line) instead of only "pi process
  exited", which read as a connection problem even when the launcher never
  connected to anything. The case it was hiding: a bundled `.app` starts with a
  minimal `PATH`, and pi's `#!/usr/bin/env node` launcher exits with
  `env: node: No such file or directory` when `node` lives under nvm, fnm,
  volta, mise, or asdf. The child `PATH` now includes those install dirs (the
  newest nvm/fnm version, and pi's own managed node), and every spawn that
  needs node — the RPC session, `pi update`, the MCP probe, the RPC patcher —
  resolves it from a `.app` launch. Settings → Runtime's stderr panel redacts
  known MCP secret values before showing the child's lines.

## [0.2.12] - 2026-10-24

### Added

- **Orbit Azure** and **Orbit Azure Light** are new themes, and they are the
  appearance default: a charcoal (or cool near-white) canvas with a restrained
  blue accent and accent-filled primary buttons. The original ember palette
  stays available as **Orbit** / **Orbit Light**, unchanged in identity —
  near-black canvas, ember accent, light primary buttons with dark ink. Saved
  preferences keep working: `orbit` still resolves to the ember palette and the
  old theme keys migrate as before.
- Primary buttons now read a dedicated fill role (`action`) rather than the
  accent, so a palette can use a neutral button fill without giving up its
  accent for selection washes, focus, tints, and accent-colored text.
- **Settings → Privacy → Temporary files** lists Orbit's scratch entries in
  its `orbit-pi` folder inside the system temp directory, with each file's size
  and age, an **Open folder** action, and **Clean up now**. The list covers
  bug-report screenshots, update downloads, and helper scripts, plus older
  top-level `orbit-*` leftovers, except entries a live Orbit process still
  needs.
- A separate **AI scratch files** group lists pi's own `pi-*` entries (shell
  output, codemode images, share/editor working files) and removes only those
  last used more than an hour ago. Neither section touches your workspace or
  pi's session store.
- **Linux arm64 builds** ship next to the x86_64 ones: a native `arm64` `.deb`
  and an `aarch64` `.tar.gz`, built on the same release run and published as
  their own update feed, so Orbit installs and updates natively on Arm machines
  instead of only on x86_64.

### Removed

- **Thirty-six ported editor themes** are gone from Settings → Appearance.
  Orbit's six palettes and **Flexoki** / **Flexoki Light** remain (plus the new
  **Orbit Azure** pair), and the theme
  dropdown now lists every palette for the mode in effect instead of hiding most
  of them behind an "All themes…" disclosure. A saved preference that names a
  retired theme still loads: dark keys fall back to Orbit, light keys to Orbit
  Light.

### Changed

- Color is a single semantic vocabulary now. `Theme` exposes the design system's
  complete surface/text/accent/status roles (`background`, `surface`,
  `surface_secondary`, `surface_elevated`, `surface_hover`, `surface_active`,
  `border`, `border_subtle`, `text_primary`, `text_secondary`, `text_muted`,
  `text_disabled`, `accent`, `accent_hover`, `accent_pressed`, `accent_fg`,
  `accent_text`, `selection`, `selection_fg`, `focus_ring`, `success`, `warning`,
  `error`, `error_fg`, `info`), and every component paints through them instead of
  the old per-palette names. The raw palette layer uses the same names, so a
  palette edit touches one place.
- Accent-colored text and icons read `accent_text` (a legible blue: `#6EA0FF` on
  charcoal, `#1F5DD8` on light) while `accent` stays the fill role; selected
  navigation and list rows paint `selection`, pressed/open controls paint
  `surface_active`, and keyboard focus paints `focus_ring` (with `button_frame` /
  `icon_button_frame` gaining a visible focus ring).
- Status colors are checked as text: light success/warning/error and dark
  Flexoki/Azure error were stepped until they clear 4.5:1 on the raised card, and
  `text_muted` was lifted in every palette so metadata on a card stops falling
  below AA. The default dark selection wash is `#2F3949` (one step below the
  reference `#303A4B`) so accent labels on the active row clear AA too.
- Disabled controls use `text_disabled` instead of a component-level
  `opacity(0.5)`, so a dimmed control's label stays predictable in both themes.
- The design-token spacing sweep is finished: every on-scale hard-coded
  padding / margin / gap in the app's chrome (top-level view, transcript row
  chrome, session strip, Sidepane, diff view, context meter, Explorer panel
  and viewer, update dialog, Skills page, custom-UI surfaces, Git panel
  widgets, dialogs) now resolves through `DynamicSpacing`, so all of it scales
  with the UI font size and Spacing Density setting like the already-migrated
  surfaces did. Optical 1–2px nudges, off-scale legacy rhythm values
  (10/14/18/22px), and bespoke geometry keep their fixed px.
- Repeated popover-width magic numbers became named constants (`BRANCH_MENU_W`,
  `ACTION_MENU_W` in the Git panel; `MODE_POPUP_W` for the Access / Workflow
  pickers), and a legacy NaN font-size value no longer panics startup — it
  migrates to the default UI font size.

### Added

- **Image attachments are compressed at ingest.** A large PNG/JPEG attached to a
  prompt is downscaled (longest edge 1400 px, never upscaled) and re-encoded as
  JPEG (quality steps down to fit a 512 KiB base64 payload), while images with an
  alpha channel become lossless WebP so transparency survives. Images already
  under both caps pass through untouched, and animated GIFs pass through so their
  frames survive. The status line reports the before/after size when an image
  shrank, and both the composer chip and bug-report screenshots use the
  compressed bytes — so a visual-debugging session can no longer balloon the
  request body (see `docs/provider-request-limits.md`).

### Changed

- **Session-store scans are O(changed files).** The sidebar's session metadata is
  now cached per file, keyed on the file's `(mtime, length)`, so a turn settle or
  a watcher reload re-reads only the session files that actually grew instead of
  rescanning the whole store (which could hold hundreds of MB). A watcher reload
  that changed nothing no longer rebuilds the sidebar or repaints.

- **The transcript's tok/s figure is labeled honestly.** It is an *average* over
  the whole provider request (prefill plus decode), so it now reads `tok/s avg`
  instead of implying instantaneous decode speed — the reason a long, mostly
  cached prompt still shows a lower number.

- **The navigation rail no longer rescans the message list every frame.** Its
  user-turn index is cached in `Transcript` and rebuilt only when the message
  list actually changes (messages only append or are rebuilt wholesale), so a
  long transcript does not pay an O(message-count) pass on every render.

### Debug

- **Per-turn diagnostic log.** Debug builds (`cargo run`) print one `[orbit:perf]`
  line to stderr when a turn settles — context window, cache read and hit rate,
  uncached prompt, output, duration, and average rate — so a drop in tok/s can be
  attributed to prefill vs. decode at a glance. This is local-only diagnostic
  output, compiled out of release builds; nothing leaves the process.

## [0.2.11] - 2026-10-21

### Added

- The new-task page's background is now a living ordered-dither field instead of a
  static dot grid. An 8×8 Bayer screen of four dot sizes over a soft density field
  covers the page edge to edge — densest in a band behind the composer, calmer but never
  empty at the window's borders and corners — and drifts slowly enough that dots cross
  their thresholds a few at a time, so
  the page breathes instead of strobing. It reads the same dither language the photo
  backdrop already speaks, costs no asset, follows the palette, and freezes under
  Reduce Motion. A configured background image still replaces it.
- The app monogram is screened into that field. Its mask is sampled from the mark the
  binary already ships (`app-icon.png`) rather than redrawn, so the backdrop cannot drift
  from the icon and a `cargo run` build never grows the Alpha pill; the glyphs hold a quiet
  density at rest and a soft band of light crosses them about every twenty seconds, filling
  each letter with the largest dot in turn. A window too small to hold the mark drops it.
- The terminal panel is tabbed. ⌘T opens another shell in the current
  workspace, ⌘W closes the active one (the panel closes with its last tab), and
  ⌘⇧] / ⌘⇧[ step between tabs — all scoped to the terminal, so Ctrl+Tab still
  reaches the shell and session cycling is untouched elsewhere. Where Ctrl+W and
  Ctrl+T belong to the shell (Windows, Linux), the same actions take
  Ctrl+Shift+W / Ctrl+Shift+T instead. A background
  tab keeps its PTY alive: a build left running in one tab keeps running while
  you work in another. Switching sessions no longer kills the shells either;
  new tabs follow the active workspace, existing ones stay where they are.
- Each tab carries its login-shell profile mark (zsh, bash, fish — the Simple
  Icons shell logos, CC0), with the shell name and start directory on hover.
  The panel also gained a proper resize handle above the tab strip, so the
  header row belongs to the tabs instead of doubling as a drag target.

### Fixed

- Sending a prompt now shows the run immediately. The composer used to stay on
  **Send** and the transcript stayed silent until pi's `agent_start` and the
  first streamed token — on a slow provider, or a pi that was still booting,
  the send looked dropped. The turn is now in flight from the moment you press
  Enter: the composer flips to Stop, the transcript shows the live
  “Working for…” row (its clock starting at the send), and a second Enter
  queues a follow-up instead of racing a second prompt. A prompt pi rejects
  unwinds that state, so nothing is left spinning.
- macOS: closing the window no longer strands the app. The Dock icon kept its
  running dot but a click did nothing — the reopen request macOS sends to a
  windowless app had no handler, so only a full quit brought a window back.
  Clicking the Dock icon (or the app's Dock tile) now opens the window again,
  on the session that was still open — the shell, the pi process behind it,
  and any stream that kept running while the window was closed all survive the
  close. The heartbeat runs on the app rather than the window, so a run that
  settles with no window open still posts its notification.

## [0.2.10] - 2026-10-16

### Changed

- Trim chrome that restates the default (#62)

## [0.2.9] - 2026-10-14

### Changed

- The composer's **model** and **thinking-level** pickers now read as one
  quiet list: option rows sit one hairline-free `Base01` step apart (matching
  the access/workflow menus) instead of a 6px gap with a hairline between
  every row, the provider scope row is a touch tighter, and each row's
  leading provider/reasoning tile stays neutral so ember marks only the
  selected row's check. A full-height popover is ~36px shorter, and the key
  legend can no longer wrap into a second line.

## [0.2.8] - 2026-10-12

### Added

- **Report a bug** from Settings → Report a bug: choose an **issue type**
  (bug / feature / other, which sets the GitHub label and the section names),
  paste the context into the field at the top, and **Generate draft** turns it
  into an issue title and description with the app's active model (falling back
  to the notes when pi is unavailable). The form attaches the build details —
  Orbit version, app ID, OS and architecture, install kind, pi CLI version, and
  the signed-in `gh` account — and creates the issue through the `gh` CLI when
  it is installed and signed in, otherwise opening GitHub's new-issue page with
  the title, body, and label prefilled. **Screenshots** can be attached too:
  they upload with `gh issue create --attach` when `gh` is signed in, and
  otherwise GitHub's web form opens with the images saved to a temp folder to
  drag in. The form is its own settings section (also reachable from the command
  palette), and its strings are localized in all ten shipped locales.

### Fixed

- Removing a workspace from the sidebar no longer leaves its heading behind.
  Workspace paths are canonicalized (symlinks resolved, so macOS `/tmp` and
  `/private/tmp` are one folder) when the project list is loaded, when a folder
  is added, and when the active workspace is set — so a session's recorded
  `cwd` always lands under its project, a legacy store that listed the same
  folder twice collapses to one entry, and one click removes the folder for
  good.

## [0.2.7] - 2026-10-09

### Added

- The Worktrees page opens with a short **How worktrees work** primer: what a
  worktree is, how to create and open one, the optional
  `.orbit/worktree-setup.sh` step, and the folder-vs-branch rule. It is
  dismissible and stays gone once closed.
- Git worktrees as first-class workspaces. A new Worktrees page (workspace
  header menu or the `Open Worktrees` command) lists every worktree Git
  reports, creates one from a new or existing branch (name, branch, and path
  are independent concepts), opens it as the active workspace — Explorer,
  terminal, Git page, Review, MCP scope, and the agent all follow its path —
  and supports rename (directory only, never the branch), move, lock/unlock,
  remove with a dirty-worktree confirmation, and advanced prune/repair. The
  status bar's **Work in** chip switches between the main working directory and
  any linked worktree and opens the create dialog with a suggested but fully
  editable (custom) worktree name; the default root is `repo/.wt`, configurable
  globally (Settings → Worktrees) and per repository (`.orbit/worktree.json`),
  and an optional setup script (default `.orbit/worktree-setup.sh`, explicitly
  allowed per repository on first use) runs asynchronously after creation with
  `ORBIT_ROOT_PATH` and `ORBIT_WORKTREE_PATH`, keeping the worktree on failure.
  Sessions record and restore the worktree path; an unavailable worktree is
  surfaced instead of silently falling back to the main repository. See
  `crates/orbit-pi/docs/worktrees.md`.

### Changed

- **Refresh** (⌘R, the File menu, or the command palette) now refreshes the
  whole app, not just the session list. It re-reads sessions and settings from
  disk, re-lists the active repository's worktrees, rebuilds the Explorer tree
  and the Review diff, refreshes Git status, re-scans the Usage analytics,
  re-reads the MCP config, and re-asks the running agent for its model/skill
  catalog, session stats, and provider auth/quota. The menu and palette entry
  is now labelled **Refresh** rather than **Refresh Sessions**.
- Quieter chrome on the work surfaces:
  - The sidebar's "star the project" card is gone — the same link stays in
    Settings → About.
  - Appearance drops the duplicate palette-swatch strip beside each theme
    picker.
  - The Usage page's cards lose their explanatory subtitles; the metric
    captions that carry data stay.
  - The status bar drops the redundant branch-count chip (the branch chip's
    own popover lists every branch) and hides the MCP indicator while every
    server is connected.
  - The composer's Plan/Build/Ask chip is the single home for the workflow
    scope: the new-task page no longer draws its own centred Mode control
    (a block of tiles in the empty state). The chip sits with the other
    agent controls on every composer, new task included, and still holds the
    choice as pending until the session id exists.
  - Settings → General states the notification trigger once — "When a run
    finishes or pi needs an answer" — instead of repeating it on each of the
    three channel rows.
  - The pull-request detail page drops its metadata rail, which only
    repeated the chips above it (review, checks, changes, branches). It now
    reads as one centred column, with the opened/updated times folded into
    the header line and the checkout/close/open actions moved below the
    review actions.
  - The pull-request list row now splits identity (number, title, labels,
    author, branches) from status (review, checks, diffstat, time), which
    reads down the right edge, and merged/closed rows carry their own state
    glyph instead of a recoloured open-PR icon.
  - The issue pages get the same treatment: the list row moves its comment
    count into the right status column, and the detail page drops its rail —
    assignees, the labels editor, and the updated time fold into the header
    as one centred reading column.
  - Inline code in the transcript keeps the body ink and its wash instead of
    painting the glyphs in the accent. A code-heavy answer used to become a
    field of ember and read as one long link; now only links carry the
    accent, and code is marked by the mono face and the chip wash.
  - The sidebar wordmark is held to a quieter size, so it labels the
    workbench instead of reading as the column's headline.
  - An expanded command tool call now reads as one terminal surface. The
    **Command** and **Output** sections sit on a single recessed code plane
    (`code_bg`) divided by a hairline, each with its label and copy control on
    a shared axis, instead of two label-plus-body groups floating on the card
    with only a gap between them. The command keeps its `$` prompt gutter, so
    a wrapped command hangs under the first line.
  - Settled tool activity is monochrome. Tool glyphs used to shift the accent
    into a per-kind hue (amber for run, cyan for explore, green for mutate,
    violet for web); a long transcript became a field of tinted badges. The
    glyph shape already names the kind, so settled work now uses one muted
    ink. Running still takes the accent and failure still takes red.
  - Transcript cards (tool, summary, thought, ask) use the plain `border`
    hairline instead of `border_strong`. `border_strong` is the focus/hover
    edge; using it at rest made every card read a step too loud.

### Fixed

- Hover fills no longer square off rounded cards. gpui 0.2.2 clips
  `overflow_hidden` to a rectangle, not the border radius, so a full-bleed
  child background painted square corners over the rounded edge. The tool
  card's header (the reported case), its error strip and detail surface, the
  summary card's header, the code-block title strip, and the MCP server rows'
  first/last entries now carry the matching corner radius themselves.
- Two `tr!` call sites bound the wrong placeholder name, so the literal token
  was printed instead of the value: the tool-detail and edit-diff fold labels
  showed `Show all %{count} lines` (the call bound `total` for a `%{count}`
  key), and a failed branch creation toasted `Branch create failed: %{err}`
  (the call bound `error`). Both now bind the declared name.
  `scripts/check_i18n.py` gained a placeholder pass so the class cannot
  recur — it validates every `tr!` call's bindings against its key's
  `%{…}` placeholders and exits non-zero on a mismatch.
- A truncated tool result no longer renders its **truncated** chip twice in
  the activity row; the chip was queued twice back-to-back.
- The transcript no longer writes a per-render `[card-dbg]` line to stderr
  for every read/edit/write tool card.
- The transcript's **tok/s** rate reads correctly again. It was measured
  client-side from the gaps between pi's streamed deltas, but pi emits the
  response in bursts, so that window collapsed to a fraction of the real
  generation and the rate ballooned (e.g. `600 tok/s` against a true ~100).
  The settled rate now uses pi's own per-call `durationMs`, and the live rate
  divides by the time since the turn's `turn_start` — the same clock basis —
  so the two figures agree.
- Baseten's provider-usage readout works again. Its billing summary requires
  an explicit ISO 8601 `start_date`/`end_date` range (31 days maximum), so the
  old `?window=30d` query returned HTTP 400 and the Providers page showed an
  error instead of a spend figure. The adapter now asks for a rolling 30-day
  window and sums the response's `dedicated_usage`, `training_usage`, and
  `model_apis_usage` totals — the shape Baseten actually returns — showing the
  combined spend with a per-category breakdown.
- The issue and pull-request detail headers no longer collapse their meta
  line to one character per line. The text beside the state chip is now
  `flex_1` + `whitespace_normal`, so it takes the column's remaining width
  and wraps there instead of shrinking inside a wrapping flex row and
  overrunning the rows beneath it.
- A review or comment card with no body (a bare approval, for example) no
  longer draws a stray hairline across its bottom — the header rule that
  separates a body from its header now appears only when there is a body.
- RPC patch auto-apply now locates the pi package for pi's **managed
  installer** as well as npm/pnpm/bun globals. The managed launcher at
  `<agent>/bin/pi` is not inside a `node_modules`, so the old ancestor walk
  never reached the package at
  `<agent>/install/releases/<version>/node_modules/@earendil-works/pi-coding-agent`
  and the Settings → Runtime row read *pi package not found*. Resolution now
  handles that layout on every platform — via `PI_MANAGED_INSTALL_ROOT`,
  `install/current-version`, or the newest release — searches the usual
  install roots when even the launcher is missing. The resolved path is
  recorded in `~/.orbit-pi/rpc-patches/config.json` (shown as **Package
  root** in Settings); the live launcher stays authoritative, so the row
  follows `pi update` to the new release dir and the saved path is only the
  fallback when no launcher resolves. Override for a single run with
  `PI_PACKAGE_ROOT`; the bundled `contrib/pi-*-rpc/apply.mjs` scripts honour it
  and fall back to the managed layout, so hand-runs work too.

## [0.2.6] - 2026-10-07

### Added

- The live **Working for…** line now shows the streaming output rate — e.g.
  **Working for 12s · 42 tok/s**. The rate is generation-only: output
  tokens over the time the model was actively decoding, so tool execution,
  round-trips, and provider stalls don't drag it down. Tokens come from the
  provider's cumulative `usage.output` when reported, with a `chars/4`
  estimate otherwise; the client-measured decode window carries through to
  the settled footer, where the `↑in ↓out · $cost` metric also shows the
  rate and the hover breakdown gains a **Rate** row.

### Fixed

- The cache hit rate no longer reads 100% for a session that wrote cache.
  pi normalizes the token buckets as disjoint, so the denominator now
  includes cache-write tokens — `cache reads / (reads + uncached input +
  writes)` — instead of only reads and uncached input. A share below the
  whole is also withheld from rounding up to a displayed `100%` (it shows
  `99.9%`), and cache rates now render to one decimal.

## [0.2.5] - 2026-10-06

### Changed

- Filled buttons now read as raised from the surface: each carries a soft
  top-lit gradient built from its own fill color plus a short contact/ambient
  shadow (`RaisedExt::raised` in `theme/tokens.rs`). Segmented controls get the
  gradient without the per-segment shadow; ghost icon buttons and disabled
  buttons stay flat.

### Fixed

- Switching to a session in another workspace no longer kills the one that is
  running. The MCP config fingerprint is workspace-specific, but the
  parked-session resume check compared it against the workspace being left,
  so every cross-workspace switch looked like an MCP change and dropped
  (killed) the parked process — truncating its stream mid-turn. The check now
  uses the target session's own workspace, and a busy run is never dropped for
  staleness (#46).
- A parked session no longer stalls mid-run when a tool asks for approval.
  Its `extension_ui_request` is now cancelled with the top-level
  `cancelled` flag pi expects; the previous payload nested it under `value`,
  so the extension kept waiting and the background run stayed `busy` forever
  (#46).

## [0.2.4] - 2026-10-02

### Added

- **⌘⇧G** opens the Git/GitHub page (and closes it again), and the **…**
  overflow menu now shows each row's shortcut — the Git row reads **⌘⇧G**.

### Changed

- The Git page's commit message box now matches the chat composer: the
  multi-line editor and its action row share one rounded, lifted surface, so
  the message reads as a text area instead of a one-line field. **Enter**
  inserts a newline and **⌘↵** / **Ctrl+Enter** commits.
- The top bar's right side now carries only what shows live state: the
  provider quota, **Open in**, the Review chip, and a single **…** menu. The
  Review chip folds together the side-panel toggle and the working tree's
  `+N −N` counts (one or the other); the new **…** menu holds **Session
  details**, **Explorer**, **Terminal**, and **Git**, each still on its
  shortcut and one click away.
- Stashes moved off the Git page's **Changes** tab onto their own **Stashes**
  tab (⌘⌥4), so **Changes** stays the review browser plus commit bar; the
  **Issues** and **Pull requests** tabs moved to ⌘⌥5 and ⌘⌥6.

### Fixed

- Switching to another session no longer kills a session that is still
  starting. A process pi has not named a session file for yet parks in a
  pending pool, keeps draining its run, and joins the warm pool under its
  file as soon as the boot handshake names it — instead of the incoming
  session's adopt dropping (and killing) it (#46).

## [0.2.3] - 2026-10-01

### Added

- Add MCP server management, settings UI, and status (#45)
- Check installed packages for updates and surface a notice (#43)

### Changed

- Feat/add star the repo button (#44)

## [0.2.2] - 2026-09-30

### Added

- Hover hints now carry the shortcut: the model and thinking chips, New Task,
  the sidebar's Search/Usage rows, the top-bar panel toggles, the Git page
  tabs, and the workspace **+** use registry-backed tooltips ("Choose Model
  (⌘⇧M)"), and hovering a session row shows the **⌘1…⌘9** slot that opens it.

- A single command registry now backs the keymap, the command palette, and
  Settings → Shortcuts, so a chord, its chip, and the action it runs can never
  drift apart. The palette gained **Rename Session**, **Pin / Unpin**, **Clone
  Session**, **Delete Session**, **Choose Model** (⌘⇧M), **Choose Thinking
  Level** (⌘⇧T), **Review Changes** (⌘⇧R), and **Keyboard Shortcuts** (⌘/).

- Session management is number- and cycle-addressable: **⌘1…⌘9**
  (**Ctrl+1…9**) open the Nth session in the sidebar's visible order, and
  **Ctrl+Tab / Ctrl+Shift+Tab** cycle to the next or previous session. The
  palette also lists **Next Session** and **Previous Session**.

- The Review pane is fully keyboard operable: `Tab` / `Shift+Tab` move between
  the workbench's focus stops, arrows walk the changed-files tree, `Enter`
  opens a file or toggles a folder, `n` / `p` jump between changed files,
  `[` / `]` jump between hunks, `e` / `c` expand or collapse every file, and
  `Escape` returns to the chat. The composer's autocomplete and the terminal
  keep `Tab` for their own use.

### Changed

- The Git page tabs moved from ⌘1…⌘5 to **⌘⌥1…⌘⌥5** (**Ctrl+Alt+1…5**) so
  the number keys can address sessions directly.

### Fixed

- Orbit and `orbit-rpc` also probe `~/.pi/agent/bin` for the `pi` launcher,
  alongside the npm/pnpm/bun home dirs they already checked. A bundled `.app`
  never inherits the shell PATH that contains it, so a pi installed there used
  to read as missing.

- Shortcuts that were advertised but macOS-only (`cmd-` editor clipboard and
  undo, Files close, Git page tabs) now use the platform modifier, so Windows
  and Linux get the same keys. The ⌘. abort chord was dead — gpui names the key
  `.`, not `period` — and now fires.

- Compaction and branch summaries now appear in the transcript as boundary
  cards: **Compaction — Compacted from 150K tokens** (click to expand the
  summary) and **Branch summary**. A successful compaction also re-reads pi's
  compacted context right away, so the summarized history is replaced by the
  summary the moment it lands instead of lingering until the session is
  reopened.

## [0.2.1] - 2026-09-29

### Changed

- Dropdown menus across the app now match the branch selector's popover
  style: one surface treatment, roomier rows with the same hover, and the
  current choice marked with the active fill and a check. This covers the Git
  page's Sync and "…" menus, changed-file and merge/rebase menus, the Review
  source menu, the Usage filter and row menus, the composer's add/mode
  popups, and the sidebar session menu; the branch, label, template, and
  Settings pickers now share the same row treatment. Right-click context
  menus stay compact.

### Fixed

- The Issue page's **Label** dropdown now closes when its trigger is clicked a
  second time. The popup dismissed on mouse-down and the trigger's mouse-up
  re-opened it on the same click; the trigger now honors the same gesture
  guard every other dropdown uses.

- **New Task** now starts a fresh session when pi has exited instead of doing
  nothing. With no turn in flight the button sent `new_session` to the live
  process, so while the reconnect banner was up the command went into the dead
  process's stdin and was never answered; it now spawns a replacement whenever
  the process is not alive, dropping the corpse and its stale exit banner. The
  sidebar's workspace **+** gets the same fix.

## [0.2.0] - 2026-09-28

### Added

- The Git page's **Issues** and **Pull requests** forms now discover the repository's own
  templates (`.github/ISSUE_TEMPLATE/` Markdown and YAML issue forms, plus
  `.github/PULL_REQUEST_TEMPLATE.md` and its variants). A lone template applies
  automatically; with several, a **Template** picker (including **Blank**) chooses one.
  The choice prefills the title prefix and body skeleton — a YAML issue form is flattened
  into Markdown — and its `labels:` ride along on **Create**.

- The Git page's **Issues** and **Pull requests** forms can now draft their title and
  Markdown body with a **Generate** button. Add optional notes (or leave them blank) and
  Orbit makes the same one-shot, tool-free `pi -p` call the commit-message generator uses,
  reading the branch, its commits, and its changed files; when pi is missing or has no
  credentials it falls back to a local heuristic built from those same facts. Nothing is
  created until you review the draft and press **Create**.

- Every icon-only button across the app now carries the same native tooltip the
  send / stop button has — a localized label, usually with its keyboard shortcut
  (e.g. `Toggle Terminal (⌘J)`). Covers the top bar, sidebar, transcript copy /
  diff controls, composer add button, window caption buttons, Explorer, Review
  pane, Git, Usage, terminal, settings, and the picker / dialog controls.

- The sidebar's Projects list now defaults to most recent activity, with a
  sort control on the header to order workspace groups by recently added, name
  (A–Z or Z–A), or session count, or fall back to the manual added order. The
  choice persists in `~/.orbit-pi/workspaces.json` and each project now records
  when it was added.

- Each project in the sidebar can now wear its own mark: pick from a curated
  set of the bundled HugeIcons and tint it with one of eight semantic colors
  (which follow the active palette). Reach it from the workspace header's `⋯`
  menu → **Icon & color**, with a Reset back to the folder default. The choice
  persists in `~/.orbit-pi/workspaces.json` beside the project list.

### Fixed

- Images in issue/PR bodies, comments, and transcripts now render. The markdown image
  element forced a definite `height: 0`, and gpui derives a missing width from a definite
  height before the image decodes (0 × ratio), so a loaded screenshot collapsed to a 0×0
  box with only its border visible. The height is left auto now: the image scales to the
  reading column and taffy keeps its aspect ratio.

- Screenshots in replies load again when GitHub stored them as signed
  `private-user-images.githubusercontent.com/…?jwt=…` URLs. That JWT expires within
  minutes, so the app got a 404 and showed the alt-text fallback; the renderer rewrites
  the URL to the durable `github.com/user-attachments/assets/<uuid>` form encoded in the
  filename, which serves the same image.

- GitHub issue/PR bodies and comments now render screenshots that GitHub stored as raw
  HTML `<img src="…">` tags when the line is inside a blockquote or list item — a quoted
  bug report is a common case. Previously only a standalone top-level `<img>` line became
  an image, so these showed the tag as text; the whole tag/its URL rendered as a link.

- The **New pull request** form's **Base branch** picker, and the **Template** pickers on
  both new-issue/new-PR forms, no longer float over the wrong part of the form or paint
  behind its fields. They are anchored to the chip that opens them (opening upward at the
  bottom of the PR form), draw above the form, and use the top branch selector's dropdown
  chip (glyph, value, chevron) and picker surface — list inset, rows, and search row for
  the branch list.

- The **Issues** tab's **Label** picker — the list filter chip and the detail view's
  **Edit labels** button — no longer opens at the page's top-right over the issue list.
  It anchors to the button that opened it, opens directly beneath it, and uses the same
  picker surface and rows as the branch selector.

- The conversation rail's active tick now follows every navigation path, not
  just wheel and scrollbar gestures. Clicking a tick and pressing ⌘↑/⌘↓ scroll
  the list programmatically, which never fired gpui's scroll handler, so the
  highlight stayed on the turn the reader left behind. Repeated ⌘↑/⌘↓ presses
  also walked a single turn and stalled; they now step through the transcript,
  and ⌘↑ from the live edge jumps back to the previous turn instead of doing
  nothing. The viewport hint wins while a run streams too, so scrolling up to
  read history moves the rail even mid-stream.

- The sidebar's project headers now stay pinned as you scroll through *any*
  expanded group, not just the active workspace. Previously only the open
  workspace's header was sticky, so a long list under the first project pinned
  while every other expanded project scrolled its header away.

- The "open in" menu's app icons are crisp and no longer tiny. macOS returns an
  icon whose largest representation is 1024×1024, and the menu embedded that PNG
  unchanged, so the renderer's single bilinear pass had to squeeze it into a
  14px slot — the blur. Icons are now rasterized to exactly the pixel size the
  menu draws (36×36 for the now-18px slot, matching a Retina blit 1:1), which
  also drops each embedded icon from roughly 1 MB to 2 KB.

## [0.0.19] - 2026-09-28

### Changed

- **The Review pane gained view controls.** The changed files collapse to their
  headers or expand back in one click (each header is its own toggle), long lines
  wrap or unwrap (unwrapping pans only the diff body — the file name and change
  counts stay fixed at the pane's edges), the diff switches between unified and
  side-by-side rows, and the pane can own the page — the session view yields and
  the sessions sidebar opens beside it — or minimize to a rail, returning to the
  docked side pane first. All icon toggles with tooltips.

### Added

- Settings → Agent → Behavior now lets Enter queue a follow-up (the existing
  default) or steer the running task. The preference persists across launches
  without changing pi’s settings or messages already queued.
- Compact Send/Steer/Queue hints on the right side of the composer footer,
  immediately left of the context-window indicator, plus send/steer/stop tooltips.
  Settings → Shortcuts reflects the selected default. Hints remain visible with
  an empty draft; the newline shortcut stays in Settings rather than adding
  clutter below the composer.
- Transcript tool cards show the file's language icon on `read` / `edit` /
  `write` rows and on the changed-files summary. `web_fetch` rows show the URL
  as a clickable link with the scheme hidden, and `web_search` rows show the
  query instead of the raw arguments JSON.
- Bare `http(s)://` URLs in user and assistant prose are links now: accent
  colored, underlined, marked with the HugeIcons external-link arrow, and
  opened in the default browser on click. Markdown `[label](url)` links keep
  their own label.

### Changed

- Alt/Option+Enter uses the opposite sending mode; ⌘⇧Enter / Ctrl+Shift+Enter
  remains an explicit steer. Idle sends still start a normal prompt, and
  Shift+Enter still inserts a new line. Steering and alternate-send shortcuts
  are scoped to the chat input so they cannot submit drafts from other fields.

### Fixed

- File icons render everywhere they are used (the Explorer tree, `@file`
  mention rows, attachment chips, and Git changed-file rows). The bundled Nerd
  Fonts symbols face was embedded in the app but never registered with the text
  system, so every devicon silently fell back to a text extension badge.

### Fixed

- Dragging the header of a window that is maximized on the primary display no
  longer locks the app up: the drag now starts the system's caption-move
  command instead of posting a non-client left-button message, which GPUI
  dispatches back into the drag region and the drag region answers with another
  post. Restoring and moving at the start of the drag behave like a native
  titlebar.
- The Git page's Back button leaves the page again: the leading controls (Back
  and the page title) now sit outside the window drag region, so pressing them
  no longer starts a window move that swallows the click. The empty bar after
  the controls still drags the window.

## [0.0.18] - 2026-09-26

### Added

- **Default session model** — Settings → Agent can pick the model and thinking level
  every new session starts on, whichever workflow mode it runs in. The choice lives
  in Orbit's own `~/.orbit-pi/session-defaults.json` (pi's global settings file is
  untouched), rides the spawn flags for a session's first turn and
  `set_model` / `set_thinking_level` RPC for every later session in a live process,
  and leaves pi's own default alone when unset. The thinking options are the levels
  pi itself derives for the selected model, so the control tracks the model rather
  than the live session. Resumed sessions keep the model in their file; the
  composer chip still changes model / thinking per session.
- **Zed design tokens** — `theme/tokens.rs` ports Zed's sizing system: dynamic
  spacing with Compact / Default / Comfortable density, text / headline / icon /
  button sizes, list, popover, context-menu, modal, tooltip, input and scrollbar
  metrics, the corner-radius scale, elevation shadows with `elevation_1/2/3`, and
  motion durations. Tokens equal Zed's default pixels at Orbit's defaults and
  follow the UI font size and Spacing Density settings.

### Changed

- Context menus (sidebar session / workspace, Explorer, Git sync / branch / file)
  now follow Zed's context-menu metrics: 200px minimum width, 23px entries,
  14px text and icons, Base04 / Base06 insets, `ListSeparator` spacing, an 8px
  radius with Zed's lighter popover shadow, and an 8px window-edge margin.
  Their sizes now scale with the UI font size.
- Tooltips use Zed's tooltip metrics: offset off the cursor, 14px text, and
  wrapping at 288px instead of overflowing on one line.
- Extension dialogs use Zed's modal layout: `ModalHeader` / section / footer
  insets, a Small headline, a 32px input field, 32px confirm buttons, and the
  four-layer modal shadow.
- The remaining floating modals — provider usage / API key / provider editor
  (Settings), the update dialog, and the custom-UI card — use Zed's
  `ModalSurface` (`elevation_3`): the four-layer shadow, an 8px radius, and
  `ModalHeader` / section / `ModalFooter` insets. The legacy heavier
  `popover_shadow` helper is gone.
- Sidebar session and workspace rows use Zed's list-item tokens: a 4px
  (`rounded_sm`) hover/selection radius, `TextSize` for title / label / metadata
  text, and `DynamicSpacing` for their padding and gaps.
- Transcript chrome follows the token scales: message-row padding, user-bubble
  radius / padding / text, tool and question card shells and headers, tool error
  strips, and the usage footer metric and breakdown card now read `TextSize`,
  `DynamicSpacing`, `Radius`, and `BufferLineHeight` instead of ad-hoc px.
- Settings chrome follows the token scales: the page header, section labels,
  grouped boards, and `setting_row` layout use `TextSize` / `DynamicSpacing` /
  `Radius`, the row separators use the 1px hairline token, and keycap chips size
  to `ButtonSize::Default` height. Setting cards and toolbars migrate next.
- Spacing is unified on `DynamicSpacing`. The ad-hoc `Theme::space` helper — which
  scaled with density but ignored the UI font size — is removed, and its ~185
  call sites (mostly the Git panel, Usage page, and Settings) now resolve through
  the token scale, so they follow the UI font size too.
- UI type is unified on `TextSize`. ~370 `.text_size(theme.ui_px(…))` sites snap
  onto the 10 / 12 / 14 / 16 scale (`XSmall` / `Small` / `Default` / `Large`); at
  most a 1px shift each. Sub-10px badge glyphs and 17px+ display headings, which
  the UI scale doesn't cover, keep their sizing.
- Corner radii are unified on `Radius`. gpui's fixed `rounded_sm/md/lg/xl` helpers
  and every on-scale `rounded(px(N))` (`2/4/6/8/12`) now resolve through the theme,
  so card, chip, and input corners also follow the UI font size.
- One-shot transitions adopt `AnimationDuration` (`Fast`, 150ms). Looping affordances
  (spinner, shimmer, streaming) and feedback timers keep their own cadences, which
  the three-value token scale doesn't cover.
- The settings page is fully on the token scales. Its side nav, headers, sections,
  rows, toolbars, select popups, cards, and modals now read `DynamicSpacing` /
  `TextSize` / `Radius` / `ButtonSize`; the last gpui spacing utilities and raw px
  literals are gone, leaving only bespoke geometry (avatar / dot / toggle sizes,
  fixed panel and modal widths).
- The Usage page (and its chart / heatmap / table / filters / tooltip modules) is on
  the same scales: all spacing, type, and on-scale radii resolve through the tokens;
  only chart and table geometry keeps its own px.
- Every input box follows the `InputField` tokens. The branch picker's create-branch
  field (previously frameless) now uses `input_field_frame`, and the Ask panel's
  custom-answer field uses `DynamicSpacing` offsets; all other single-line inputs
  already routed through `input_field_frame` / `picker_search_frame`. The main
  composer, file editor, and inline tree rename stay bespoke components.
- Every search box is the picker search row (`picker_search_frame`), matching the
  command palette: the settings / Git / Usage searches and the skills filter were
  boxed `InputField`s and now share the palette's flat 36px row, so all searches
  read identically. The Git panel's issue / pull-request search grows to fill its
  filter bar and shrinks when tight (min 160px) instead of a fixed 200px, so the
  Pulls tab no longer overflows with its extra state chip.
- The session-details title (rename) input follows the `InputField` tokens: its
  block uses `DynamicSpacing` / `input::gap` / `input_label` / `input_field_frame`,
  and the Generate-title and Update buttons size to `ButtonSize::Large` (32px) so
  the row lines up with the 32px field.
- Picker ↑/↓ now scroll the focused row into view. The command palette,
  workspace picker, and branch picker set a **positive** `ScrollHandle` offset,
  but gpui stores it **negative** once scrolled down (the model selector already
  negated it) — so the list never actually scrolled and the highlight walked
  off-screen, looking like ↑/↓ did nothing. They negate it now, and the palette
  also scrolls against its actual (window-capped) list height instead of the
  uncapped `list_max_h`. Two command-palette tests cover the ↑/↓ move and its
  scroll-into-view; the scroll test fails on the old sign.
- The model picker had the same ↑/↓-doesn't-stick symptom for four reasons,
  all now fixed. (1) Its open-time "pin the highlight to the active model"
  re-ran on **every** render for the first 400 ms (the hover-suppression
  window), so a ↓ pressed right after opening was silently snapped back; the
  pin now runs only on a genuine open / catalog change. (2) Its deferred scroll
  re-targeted the active model for up to 250 ms after open; it now follows the
  current highlight. (3) pi re-reports the whole catalog on every `get_state`,
  and `set_catalog` re-armed the pin and re-scrolled to the active model each
  time — so any periodic sync undid an ↑/↓ while the popup was open. An
  unchanged `set_catalog` is now a no-op. (4) The pin stayed armed until the
  popup's *first* render, so a ↑/↓ arriving before that frame (or right after a
  scope change / catalog refresh re-armed it) was undone by it; a deliberate
  ↑/↓ now cancels the pin. Covered end to end as well: a test opens the real
  `OrbitApp` with the real `bind_keys`, asserts the popup's filter actually
  holds focus, and steps the highlight through several presses.
- Picker rows no longer move the keyboard highlight on hover: the command
  palette, workspace picker, and extension-dialog option rows used `on_hover` to
  follow the pointer, and `on_hover` re-fires whenever hit-testing changes — so
  scrolling a row under a stationary pointer hijacked the highlight. They use
  `on_mouse_move` now, so only real pointer movement moves it.
- Single-line inputs no longer wrap. Every `ComposerInput` capped at
  `with_max_lines(1)` — the session-details name, the Git branch field, the
  provider / model / plugin / skill filters, the usage searches, the extension
  dialog, and the find bar — now also sets `with_wrap(false)`, so a long value is
  clipped at the edge instead of wrapping into a second row and painting the
  editor's vertical scrollbar. Non-wrapping text also scrolls horizontally to
  follow the caret, so typing past the field's width still shows what you type
  (clipped to the text area, so the code editor's gutter stays put).
- The Git panel's tab strip is on the §5 button language: each tab is a
  `button_frame(ButtonSize::Medium)` — the same 28px height, `Base08` padding,
  `Base04` gap, `button::RADIUS` (4px), and Default label as the branch chip and
  the panel's filter chips — so the tabs and the chips beside them read as one
  system. The tab strip and branch row are `Base40` tall, and the whole panel's
  spacing resolves through `DynamicSpacing` (one shared `tab_bar` for Changes /
  History / Graph / Issues / Pulls).
- The sidebar's New Task, Search, and Usage controls share one
  `button_frame(ButtonSize::Large)`: the hand-rolled 28px Search / Usage rows
  now match the New Task button's height, `Base08` padding, `button::RADIUS`
  (4px), and Default label, while keeping their ghost treatment (no fill or
  border, hover only) — one evenly sized stack.
- The model selector's option rows keep the picker's two-line token height. A
  capped list is a flex column, so without `flex_none` every row shrinks toward
  the `picker_entry` minimum (31px) instead of `picker::two_line_entry_height`
  (50px), leaving the 28px leading provider / thinking chip with almost no
  vertical padding. The no-match empty row, its provider-header gap, and the
  row's hover highlight are on the same tokens / `on_mouse_move` as the other
  pickers.

### Fixed

- The model and thinking chip pickers keep keyboard focus when opened from
  their composer chip. The composer box's own mouse-up handler (which focuses
  the input) is an ancestor of the chip and ran after it in gpui's bubble
  order, so ↑/↓/Enter/Escape went to the composer instead of the popup. The
  composer box now leaves focus alone while any of its popovers is open, and a
  click inside the model popup no longer bubbles out to it.
- The top-bar provider-quota popover no longer clips its last provider card
  when several accounts are connected: the card list scrolls inside the
  popover's height cap instead of stretching past it. The list now carries its
  own max height rather than living in a `flex_1` body — inside the deferred,
  anchored popover the available height is zero, where a flexible child
  collapses and the overflow is clipped.

## [0.0.17] - 2026-09-25

### Added

- **Workflow modes** — plan, build, or answer instead of one undifferentiated
  chat. A session is scoped **Plan**, **Build**, or **Ask** from the New Task
  page or the composer chip, per session. Plan and Ask are read-only: the
  bundled `orbit-workflow-extension` disables the write tools, gates `bash` to
  a read-only allowlist, and injects mode guidance, re-arming live sessions
  from `~/.orbit-pi/workflow.json` with no restart.
- **AI review agent** — a read-only reviewer over the current change set or the
  whole project, from the Review pane's sparkles menu or the command palette.
  It runs on its own pi process scoped to Ask mode (so the chat session and
  transcript are untouched), launched with `ORBIT_REVIEW=1` so the access guard
  never blocks its read-only `git diff`. Findings parse from a fenced JSON
  block into severity-chipped rows that scroll the diff to the offending file;
  the answer's prose is kept when no findings parse.

## [0.0.16] - 2026-09-24

### Added

- Add structured data, harden headers, and extend cache windows
- Rebuild landing around story sections and refresh OG

### Changed

- Feat/UI refinement (#23)

## [0.0.15] - 2026-09-23

### Added

- Per-tool glyph badges in the transcript, tinted by work kind, with a
  chip-style folded activity group
- Surface pi's capped-result facts on tool cards as a `truncated` chip
- Success check on the session-details Update button after a rename commits

### Changed

- Refactor the transcript UI: readable tool labels, a real shared spinner,
  shared button hover/press feedback, and redesigned git issue/PR details

## [0.0.14] - 2026-09-23

### Changed

- Auto-apply pi RPC patches and harden custom UI surfaces

## [0.0.13] - 2026-09-22

### Changed

- Feat/GitHub page features (#21)
- Feat/main updates (#20)
- Feat/GitHub page features (#19)
- Remember Open in preferences per workspace and add Rider support (#17)
- Update GitHub Sponsors username in FUNDING.yml (#18)
- Update GitHub Sponsors username in FUNDING.yml (#16)

## [0.0.12] - 2026-09-22

### Added

- Add ten new light palettes and toggle_knob role

### Fixed

- Keep the update modal's footer inside the card

## [0.0.11] - 2026-09-22

### Contributors

### Added

- **Ten more palettes** — the appearance catalog grows from 32 to 42. Two new
  Orbit-family light palettes join the set: **Orbit Paper**, a cooler neutral
  paper canvas that is the light counterpart to Orbit's warm off-white (same
  ember accent), and **Orbit Contrast**, a near-white, high-contrast palette
  with deeper ink and a stronger hairline for maximum legibility. The curated
  light counterparts of the editor themes also land — **Ayu Light**,
  **Catppuccin Latte**, **Flexoki Light**, **Gruvbox Light**, **Nord Light**,
  **One Light**, **Solarized Light**, and **Tokyonight Light** — so light is a
  family in the Appearance dropdown rather than a single option. The picker
  partitions the catalog by mode, so every choice it offers matches the active
  mode.

- **`toggle_knob` palette role** — a mode-aware token for the knob of a toggle
  switch, kept light in both modes so it reads on the accent (on) and the
  raised track (off). The knob previously used `text`, which painted a black
  dot on the light track in light mode.

### Changed

- The saved theme keys `one-light` and `flexoki-light` now resolve to the real
  **One Light** and **Flexoki Light** palettes instead of being folded into
  Orbit Light; the generic `light` alias still maps to Orbit Light.

### Fixed

- Clicking a radio row no longer leaves a stray accent focus ring behind.
  gpui 0.2 has no `:focus-visible`, so the automatic focus transfer on
  mouse-down is suppressed; the ring appears only for keyboard focus, while
  the row's own click still selects on mouse-up.

- **Dumitru Moloșnic** ([#11](https://github.com/imrj05/orbit/pull/11)) — light,
  dark, and system appearance modes; transcript table sizing, streaming
  scroll-position, and multiline command-preview fixes.

## [0.0.10] - 2026-09-21

### Added

- **Explorer** — a Zed-style file-and-folder explorer for the active workspace.
  A left **project panel** (`cmd-shift-e`, top-bar folder toggle, or the ⌘P
  row) renders the workspace as a virtualized tree: expand/collapse,
  case-insensitive filter, a hidden-files toggle, devicon glyphs, `M/A/D/R/U`
  git-status badges, and keyboard navigation (arrows, home/end,
  `cmd-←`/`cmd-→`, enter/space). The walk is gitignore-aware (nested
  `.gitignore`/`.ignore`, global and repo excludes; symlinks never followed)
  and runs off-thread with a bounded entry cap surfaced honestly as
  "truncated". Right-clicking a row offers Open, Open in Default App, Reveal
  in File Manager, and Copy Path / Relative Path. Clicking a file opens a
  full-page **Files** editor: a tab strip over syntax-highlighted
  code with line numbers, editable Markdown (with an Edit/Preview toggle), or
  an image, with a path/size/line-count toolbar and a save-status chip. Code,
  text, and Markdown files are editable and autosave to disk (debounced;
  `cmd-s` saves now); unsaved tabs carry a dot, and external changes are
  flagged rather than silently overwriting edits. Binary files, oversized
  files (2 MiB), truncated files, non-UTF-8 files, and directories get an
  honest notice instead of a lossy dump, and never enter the editor. The
  workspace watcher refreshes the tree and the open file on change. `cmd-w`
  closes the active tab (the last tab, or `cmd-shift-w`, closes the surface),
  and the toolbar X returns to the chat.

- **Editing** — the Explorer's editor (and the chat composer) supports
  **undo/redo** (`cmd-z` / `cmd-shift-z`), where a run of typing within 400 ms
  coalesces into a single undo step and a caret move starts a new one. History
  is bounded by both step count and total bytes so a whole-file buffer cannot
  balloon memory.

- **Explorer file operations** — create, rename, and delete files and folders
  from the project panel. **New File** and **New Folder** sit in the panel
  header and in a directory's right-click menu; **Rename** edits a row's name
  in place; **Delete** asks first and moves the item to the OS Trash on macOS
  (permanent delete elsewhere, with the confirmation worded to match). Names
  are validated before anything touches disk, a create never clobbers an
  existing file, and a rename refuses an occupied name. An operation is refused
  while an open Files tab under the target has unsaved edits, so it can never
  race an autosave or silently discard a buffer; a renamed file's open tab
  follows the new path, and a deleted one closes.

- **More code fonts** — the Code font picker now offers seven more bundled
  faces alongside the existing set: **DM Mono**, **IBM Plex Mono**,
  **Inconsolata**, **Noto Sans Mono**, **Space Mono**, **Anonymous Pro**, and
  **Martian Mono**. Like the rest of the catalog they ship as subset static
  TTFs, so a picked face resolves without an OS dependency.

### Fixed

- Explorer rows now fill the panel width, matching the Review pane's file tree.
  The selection/hover highlight no longer collapses to a narrow pill around the
  devicon, and git badges align to the right. This also fixes the inline rename
  and new-file prompts, which rendered as an empty bubble: the prompt input is
  a `ComposerInput`, which has no intrinsic width, so on a content-sized row
  its `flex_1` collapsed to zero. A regression test renders the real panel and
  asserts the prompt input keeps a usable width.

- The Explorer's row context menu opens at the pointer, flipping above/left
  when it would overflow the window. It was pinned to the panel's top-left, so
  a row's menu always appeared at the top of the tree regardless of where the
  row was right-clicked.

- Closing a file tab keeps keyboard focus inside the Files surface: it moves to
  the newly active editor, or to the surface itself for preview/read-only tabs,
  instead of being dropped with the removed editor. Previously focus was lost
  after the first close (when the next tab did not paint an editor), so `cmd-w`
  stopped working. Closing a tab to the left of the active one also selected the
  wrong neighbour.

- Search and filter placeholders (provider, model, settings, plugins, skills,
  side pane, git panel, usage, in-transcript find, branch/workspace pickers)
  now follow a language change immediately instead of keeping the language
  they were created in. Placeholders are resolved from translation keys at
  paint time.
- With no saved language preference, `System` now walks the OS's ordered
  preferred-language list and picks the first language Orbit ships (falling
  back through unshipped choices such as Hindi) instead of defaulting to
  English. The macOS bundle also declares its supported localizations.
- The setup page's dependency descriptions follow a language change without
  re-probing the host.

## [0.0.9] - 2026-09-21

### Added

- **Version History** in the update modal, opened from the update and
  up-to-date dialogs: every release the feed carries, newest first, each with
  its changelog and the running build marked **Current**. Appcasts now
  accumulate across releases (the newest 20 signed items), so the list grows
  with each release.

### Changed

- The update modal leads with the app icon, checks against an indeterminate
  progress bar, and names the current version when up to date.
- **Check for Updates** always opens the modal. A build that can't check
  (debug, or a binary outside a managed install) says so there instead of
  posting a toast; with `ORBIT_FORCE_UPDATER=1` and the signing key compiled
  in, such a build runs a real check-only flow without **Update now**.
- The update flow now runs through a single modal. A staged release and
  **Check for Updates…** both open it: the check shows a search progress bar,
  then the release's changelog with **Cancel** / **Update now**, while an
  already-staged release offers **Later** / **Update now**. The Settings
  update control is an icon-only download button, and the sidebar shows a
  20px download glyph that expands to **Update** on hover (the reference
  app's pattern); the changelog rides in each appcast item's `<description>`
  (written from `CHANGELOG.md` at release time).

## [0.0.8] - 2026-09-20

## [0.0.7] - 2026-09-20

### Added

- Interface localization via `rust-i18n`, shipped with ten locales — English,
  Simplified Chinese, Japanese, Korean, Spanish, French, German, Brazilian
  Portuguese, Russian, and Italian — plus a `System` option that follows the
  OS preferred language. Settings → Appearance → Language switches it live
  (including the native menu bar); `locales/en.yml` is the source of truth and
  `scripts/gen_locales.py` regenerates the other files from the glossary.

## [0.0.6] - 2026-09-19

### Added

- Clone a session straight from the sidebar — each session row's `…` menu
  gained **Clone session**, which duplicates that session on disk (fresh id,
  pi's `<timestamp>_<id>.jsonl` naming, entries copied verbatim) and drops the
  copy into the list to branch off. Unlike Delete, it only reads the source, so
  it works on any row — including one with a live pi process — without
  switching away from what you're doing. The ⌘P "Clone Session" command still
  clones the open session through pi.
- Usage page — a full-width **Daily activity** calendar heatmap: one cell per
  day across a trailing 12 months, shaded by the active metric (tokens,
  requests, cost, …), with month/weekday axes, a less→more legend, a hover
  readout, and click-to-scope to a single day (click again to restore the
  range). It is independent of the date range — a contribution graph needs a
  year to read — but the active workspace / model / provider / errors filters
  apply to every cell.

### Fixed

- App and composer shortcuts now bind GPUI's cross-platform `secondary`
  modifier (Cmd on macOS, Ctrl on Windows/Linux). They previously used
  `cmd-*`, which GPUI maps to the Windows/Super key on Windows, so
  copy/paste/cut/select-all and every other primary shortcut did nothing
  there. Shortcut hint chips now render the platform-correct label too.

## [0.0.5] - 2026-09-17

### Added

- Windows releases attach the bare `orbit-pi.exe` alongside the `.zip`, so the
  executable is a direct single-file download (`scripts/bundle-windows.ps1`).
- Linux releases attach a native `.deb` alongside the `.tar.gz`, installable
  with `apt install ./orbit-pi_*.deb` (`scripts/bundle-linux.sh`).

## [0.0.4] - 2026-09-17

## [0.0.3] - 2026-09-16

### Added

- Integrated terminal — a real login shell in a resizable bottom panel (⌘J or
  the top-bar toggle), independent of the right side pane. Built on
  `alacritty_terminal` for PTY and VT/ANSI emulation, rendered natively by
  GPUI: scrollback, click-drag selection with ⌘C/⌘V, bracketed paste, a
  blinking cursor, and a per-theme ANSI palette. The shell follows the active
  workspace and restarts when it changes.

## [0.0.2] - 2026-09-16

### Added

- Native macOS menu bar (Orbit / File / Edit / View) wired to the app's
  existing actions; ⌘N and the About surface now have a real home.

## [0.0.1] - 2026-09-14

### Added

- Chat-style `pi` sessions — streaming transcript, model selection, thinking
  effort, follow-up queueing, mid-run steering, and cancel.
- Sessions grouped by project, reopenable and clonable, with an Orbit-owned
  project list that leaves `pi`'s own session store untouched.
- Access guard — Supervised / Auto-accept edits / Full access, enforced by a
  bundled `tool_call` extension with an inline Allow once / Always allow this
  tool / Deny bar.
- Review pane with a live `git diff HEAD`, and a Git page for Changes /
  History / Graph with staging and commit.
- Workbench pages — usage, skills, plugins, models, providers, and appearance,
  read from `pi`'s on-disk data.
- In-transcript find (⌘F) and a full-window image lightbox.
- Native macOS app bundle, Developer-ID signed and notarizable.

[Unreleased]: https://github.com/imrj05/orbit/compare/v0.2.21...HEAD
[0.0.1]: https://github.com/imrj05/orbit/releases/tag/v0.0.1
[0.0.2]: https://github.com/imrj05/orbit/releases/tag/v0.0.2
[0.0.3]: https://github.com/imrj05/orbit/releases/tag/v0.0.3
[0.0.4]: https://github.com/imrj05/orbit/releases/tag/v0.0.4
[0.0.5]: https://github.com/imrj05/orbit/releases/tag/v0.0.5
[0.0.6]: https://github.com/imrj05/orbit/releases/tag/v0.0.6
[0.0.7]: https://github.com/imrj05/orbit/releases/tag/v0.0.7
[0.0.8]: https://github.com/imrj05/orbit/releases/tag/v0.0.8
[0.0.9]: https://github.com/imrj05/orbit/releases/tag/v0.0.9
[0.0.10]: https://github.com/imrj05/orbit/releases/tag/v0.0.10
[0.0.11]: https://github.com/imrj05/orbit/releases/tag/v0.0.11
[0.0.12]: https://github.com/imrj05/orbit/releases/tag/v0.0.12
[0.0.13]: https://github.com/imrj05/orbit/releases/tag/v0.0.13
[0.0.14]: https://github.com/imrj05/orbit/releases/tag/v0.0.14
[0.0.15]: https://github.com/imrj05/orbit/releases/tag/v0.0.15
[0.0.16]: https://github.com/imrj05/orbit/releases/tag/v0.0.16
[0.0.17]: https://github.com/imrj05/orbit/releases/tag/v0.0.17
[0.0.18]: https://github.com/imrj05/orbit/releases/tag/v0.0.18
[0.0.19]: https://github.com/imrj05/orbit/releases/tag/v0.0.19
[0.2.0]: https://github.com/imrj05/orbit/releases/tag/v0.2.0
[0.2.1]: https://github.com/imrj05/orbit/releases/tag/v0.2.1
[0.2.2]: https://github.com/imrj05/orbit/releases/tag/v0.2.2
[0.2.3]: https://github.com/imrj05/orbit/releases/tag/v0.2.3
[0.2.4]: https://github.com/imrj05/orbit/releases/tag/v0.2.4
[0.2.5]: https://github.com/imrj05/orbit/releases/tag/v0.2.5
[0.2.6]: https://github.com/imrj05/orbit/releases/tag/v0.2.6
[0.2.7]: https://github.com/imrj05/orbit/releases/tag/v0.2.7
[0.2.8]: https://github.com/imrj05/orbit/releases/tag/v0.2.8
[0.2.9]: https://github.com/imrj05/orbit/releases/tag/v0.2.9
[0.2.10]: https://github.com/imrj05/orbit/releases/tag/v0.2.10
[0.2.11]: https://github.com/imrj05/orbit/releases/tag/v0.2.11
[0.2.12]: https://github.com/imrj05/orbit/releases/tag/v0.2.12
[0.2.13]: https://github.com/imrj05/orbit/releases/tag/v0.2.13
[0.2.14]: https://github.com/imrj05/orbit/releases/tag/v0.2.14
[0.2.15]: https://github.com/imrj05/orbit/releases/tag/v0.2.15
[0.2.16]: https://github.com/imrj05/orbit/releases/tag/v0.2.16
[0.2.17]: https://github.com/imrj05/orbit/releases/tag/v0.2.17
[0.2.18]: https://github.com/imrj05/orbit/releases/tag/v0.2.18
[0.2.19]: https://github.com/imrj05/orbit/releases/tag/v0.2.19
[0.2.20]: https://github.com/imrj05/orbit/releases/tag/v0.2.20
[0.2.21]: https://github.com/imrj05/orbit/releases/tag/v0.2.21
