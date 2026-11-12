import type { Metadata } from "next";
import { Callout, DocsPage } from "@/components/docs-page";

import { pageLocale } from "@/i18n/page";
import { localeAlternates } from "@/i18n/meta";

export async function generateMetadata({
  params,
}: PageProps<"/[locale]/docs/worktrees">): Promise<Metadata> {
  const locale = await pageLocale(params);

  return {
    title: "Worktrees",
    description:
      "Run parallel branches as first-class Orbit workspaces: create, open, rename, move, lock, and remove Git worktrees, with an optional setup script.",
    alternates: localeAlternates(locale, "/docs/worktrees"),
  };
}

export default async function WorktreesPage({
  params,
}: PageProps<"/[locale]/docs/worktrees">) {
  const locale = await pageLocale(params);

  return (
    <DocsPage
      locale={locale}
      current="/docs/worktrees"
      title="Worktrees"
      intro="A Git worktree is a second working directory backed by the same repository, checked out to its own branch. Orbit treats each one as a first-class workspace for sessions on different branches."
    >
      <h2>What a worktree is</h2>
      <p>
        Worktrees let you check out more than one branch at a time from a single
        repository. In Orbit, opening a worktree makes it the active workspace:
        the Explorer, terminal, Git page, Review pane, and every new session all
        run inside it, on its branch.
      </p>

      <h2>Names, branches, and paths</h2>
      <p>Orbit keeps three identities separate:</p>
      <pre>{`name    113
branch  feature/issue-113
path    ~/Projects/my-project/.wt/113`}</pre>
      <ul>
        <li>
          The <strong>name</strong> is what you choose; it only needs to be one
          safe path component.
        </li>
        <li>
          The <strong>branch</strong> is the Git branch checked out there.
        </li>
        <li>
          The <strong>path</strong> starts at{" "}
          <code>&lt;worktree root&gt;/&lt;name&gt;</code> and can be moved later.
        </li>
      </ul>

      <h2>Creating one</h2>
      <ol>
        <li>
          Open the <strong>Worktrees</strong> page from the command palette or
          the workspace menu, or choose <strong>New worktree</strong> from the
          status bar&apos;s <strong>Work in</strong> picker.
        </li>
        <li>
          In the create dialog, enter a name and pick an existing branch or
          create a new one. A new branch defaults to a slug of the name; both
          are editable. A remote-tracking ref creates a local tracking branch.
        </li>
        <li>
          Create it. Orbit runs <code>git worktree add</code> and, by default,
          opens the result as the active workspace.
        </li>
      </ol>
      <p>
        The Worktrees page also lists every linked worktree and supports{" "}
        <strong>open</strong>, <strong>rename</strong>, <strong>move</strong>,{" "}
        <strong>lock</strong> / <strong>unlock</strong>, <strong>remove</strong>,
        and the repository-wide <strong>prune</strong> and{" "}
        <strong>repair</strong> actions.
      </p>

      <h2>Where worktrees are created</h2>
      <p>
        New worktrees go under the worktree root, which defaults to{" "}
        <code>.wt</code> inside the repository. Precedence:
      </p>
      <ul>
        <li>
          <strong>Per repository</strong> — <code>.orbit/worktree.json</code>{" "}
          overrides the directory and setup script for that project.
        </li>
        <li>
          <strong>Global</strong> — <code>~/.orbit-pi/worktrees.json</code> holds
          the default directory and setup script, plus the automatic-create
          options.
        </li>
      </ul>
      <pre>{`{
  "directory": ".wt",
  "setup_script": ".orbit/worktree-setup.sh",
  "run_setup_auto": true,
  "open_after_create": true
}`}</pre>
      <p>
        <code>~</code> expands to your home directory, and absolute paths are
        allowed. The repository file can set only <code>directory</code> and{" "}
        <code>setup_script</code>; the automatic-create booleans are global.
      </p>

      <h2>Setup scripts</h2>
      <p>
        A repository can carry a setup script that runs inside the new worktree
        right after it is created — linking a <code>.env</code>, sharing{" "}
        <code>node_modules</code>, warming a build cache, and so on. Orbit
        hardcodes none of that; the script is entirely yours.
      </p>
      <p>The script receives two environment variables:</p>
      <pre>{`ORBIT_ROOT_PATH=/Users/you/project
ORBIT_WORKTREE_PATH=/Users/you/project/.wt/113`}</pre>
      <ul>
        <li>
          The default path is <code>.orbit/worktree-setup.sh</code>, configurable
          per repository and globally.
        </li>
        <li>
          It runs on a background executor and its output is captured, so the UI
          stays responsive. You can retry it or open the worktree even if it
          fails — <strong>a failed setup never removes the worktree</strong>.
        </li>
        <li>
          Scripts run arbitrary commands, so the first run in a repository is an
          explicit confirmation.
        </li>
      </ul>

      <Callout tone="warn">
        Only add a setup script to repositories you trust. It executes with your
        user account, inside the new worktree.
      </Callout>

      <h2>Switching workspaces</h2>
      <p>
        In a repository, the status bar&apos;s <strong>Work in</strong> chip
        offers <strong>Local</strong> (the main checkout), linked worktrees,
        recent folders, and actions to create or reconnect a worktree, work from
        a remote branch, clone a repository, or choose another folder. Choosing
        a different location starts a task there: a running session is parked,
        and an idle session&apos;s transcript remains on disk. The new-task
        page offers the same choices; its remote branch action opens the create
        dialog. Outside a repository, the status bar shows a folder chip, while
        the new-task picker still offers cloning and folder selection.
      </p>
      <p>
        Cloning asks for a URL, a folder name, and a destination directory. Git
        runs in the background with progress and cancellation. A successful
        clone opens as a working folder and appears in the sidebar; a failed or
        cancelled clone is not opened, though Git may leave a partial directory.
        The Windows job-object cancellation path runs on the Windows CI leg;
        manual GUI verification remains.
      </p>
    </DocsPage>
  );
}
