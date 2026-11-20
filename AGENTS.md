# govee2mqtt, mnbf9rca fork

A personal fork of [wez/govee2mqtt](https://github.com/wez/govee2mqtt), the bridge between Govee devices and Home Assistant. The owner runs it as a Home Assistant app for the lights in one house.

This file is background and working rules for agents. Task detail lives in the brief or spec for that task, not here.

## Why the fork exists

Upstream has merged nothing since April 2026 and its open pull requests go unreviewed. The owner needs features and fixes that will not land there, so they run this fork. Three consequences govern every change:

- **Nothing goes upstream.** No pull request, issue or comment on `wez/govee2mqtt` or on any repository other than `mnbf9rca/govee2mqtt`. Anything posted elsewhere is public under the owner's name and needs their explicit approval, one item at a time.
- **Keep the difference from upstream small.** Every line that differs is a line to carry through any future merge from upstream. Change what the task needs. Do not reformat, rename, reorganise or "improve" upstream code in passing.
- **This is one household's bridge, not a product.** Rigour is matched to those stakes. Keep what protects the lights from breaking; drop staged rollouts, compatibility layers and options nobody asked for. Fix forward when something breaks.

## Three seats

Multi-agent work here uses three seats, identified by harness type in this repository's Herdr workspace, not by tab name.

| Seat | Who | Responsibility |
|---|---|---|
| Chief of staff | The Claude session the owner is talking to | The conversation with the owner, working out what they mean, deciding what kind of work it is, delegating, ruling on disagreements |
| Builder | The Codex agent | Specifications and plans where a task needs them, code, documentation, commits, pull requests |
| Reviewer | The other Claude agent | Independent review and read-only verification |

If `herdr agent list` shows no candidate or more than one candidate for a seat in this workspace, the chief of staff asks the owner which to use. It does not guess. Generic subagents do not replace the builder or the reviewer.

**The chief of staff interprets; it does not relay.** It works out what the owner's words mean for this request, replays that reading in a few plain sentences, confirms it when the matter is complex, and then acts at that scope. It never passes the owner's words on verbatim.

**Nothing is dispatched until the owner has agreed the brief.** A question, a sketch or a correction from the owner is not agreement. "Agreed", "go" or an equivalent is.

**The chief of staff keeps its context clean.** It does not read diffs or write code. It reads verdicts and check results.

**An owner ruling beats a review finding.** Once the owner has chosen an approach, a finding against it is input for improving that approach, not grounds to reopen the decision.

## Scaling the process to the change

- **Small change** (a fix, one setting, documentation, packaging): the chief of staff writes a short brief; the builder makes the change and opens a pull request; the reviewer reviews the pull request once. No separate specification, plan or ledger.
- **Larger change** (a new subsystem, taking in someone else's unmerged work, anything that changes how existing devices are controlled): the builder writes a short specification first, the reviewer reviews it, the chief of staff approves it, then the builder implements.

When in doubt, use the small path and say so.

## Handovers and review

- Briefs, reports and verdicts are files under `.superpowers/sdd/<task>/` in the main checkout. Hand over an absolute path, never pasted output. That folder and `.claude/` must stay untracked: list them in `.git/info/exclude`, not in `.gitignore`, so the fork's tracked files stay close to upstream.
- The builder sends review requests straight to the reviewer with `herdr agent prompt`, and the reviewer replies straight to the builder. The chief of staff is not in that loop.
- The reviewer reads the change from GitHub (`gh pr diff`, `gh pr view`, `gh pr checks`), because one seat's sandbox may not be able to read another's worktree.
- A review gives one ranked report with two separate verdicts: does it meet the brief, and is the code sound. It also says what could be deleted, and what deleting it would lose.
- At most three fix rounds. All findings of a round are fixed in one commit. Only Critical and Important findings go round again. A Minor finding that is easy to fix is fixed in the same round. Easy means a few lines, inside the pull request's own scope, with no change to how lights are controlled and no decision needed from the owner. The remaining Minor findings are listed in the pull request body and in one tracking issue per pull request, as a checklist that links the pull request. A finding still disputed after two rounds goes to the chief of staff as a file path, and its ruling is final.
- The builder tells the chief of staff one line at start, at completion and on any escalation. No per-round updates.
- Never block a turn on another seat for more than about a minute. Start a background watch and carry on or end the turn.
- `herdr agent prompt` types into the other agent's pane. If that pane's owner is mid-sentence, the message lands inside their text. Keep these messages to one or two lines.

## Git and pull requests

- **Work in a worktree, never in the main checkout.** Create it with your harness's own worktree tool: Claude Code puts it under `.claude/worktrees/<name>`, Codex under `~/.codex/worktrees/`. Nowhere else: not `/tmp`, not a scratch directory, not inside another repository. Whoever opened a worktree removes it when its pull request merges or the work is dropped.
- The main checkout is shared. Run `git branch --show-current` before you commit there.
- **`gh` defaults to the parent repository in a fork.** Every `gh pr` and `gh issue` command carries `--repo mnbf9rca/govee2mqtt`. A pull request opened without it lands on upstream, in public, under the owner's name.
- One purpose per pull request. Each fix taken from someone else's unmerged work is its own pull request and names its source.
- Before each commit to a branch held open across a session, run `git fetch origin` and rebase onto `origin/main`. A stale branch reverts whatever merged since it was cut.
- Read the checks before any merge: `gh pr checks <n> --repo mnbf9rca/govee2mqtt`. A check that has not reported counts as a refusal.
- The owner merges, unless they ask an agent to. The owner approves every tag push, because a tag publishes a release.
- No attribution lines in commits or pull request bodies.
- Pushing a change to `.github/workflows/` needs the `workflow` permission on the `gh` sign-in. If a push is refused for that reason, report the exact error. Do not try another credential.

## Testing and verification

- A dependency update made by hand uses only versions published at least 7 days ago, the same cooldown Dependabot applies; only the smallest version that fixes a known advisory may be newer.
- This machine may have no Rust toolchain. Do not install one. `.github/workflows/pr.yml` runs `cargo build --all`, `cargo test --all` and `cargo fmt --all -- --check` on every pull request to `main`, and that is the test run. Say plainly when something is read but not compiled.
- The `test-addon` job builds the app image from the fork's published `:latest` image. It fails until the first build on `main` has published that image. Report that as expected; do not work round it.
- **A claim about what a light does is proved on a light.** Agents in this repository cannot reach the lights or Home Assistant. Real-device checks are done by the owner with the chief of staff. Do not report device behaviour as verified because the code reads correctly or a test passes.
- Changes to how existing scenes, colours or power commands are encoded or sent need the owner's agreement first. They can break lights that work today, and no test here would show it.

## Releases

Read `.github/workflows/build.yml` and `scripts/` before a release and confirm these points still hold.

- A push to `main` publishes `ghcr.io/mnbf9rca/govee2mqtt:latest`. A tag beginning `20` publishes the Home Assistant app images at that version, and builds them from `:latest`. So the `main` build must pass before a tag is pushed.
- The version in `addon/config.yaml` on `main` must equal the tag. Home Assistant reads the version from that file and downloads the image with that label.
- On macOS, `scripts/apply-tag.sh` fails because it uses the GNU form of `sed -i`, and `scripts/tag-release.sh` needs Docker for the changelog. Make the two edits by hand and commit as `Tag <version>`.
- The published packages must be public, or Home Assistant cannot download them.

## Sandboxes

Every seat runs inside Agent Safehouse, a macOS sandbox the owner launches agents through. You are inside it when `APP_SANDBOX_CONTAINER_ID=agent-safehouse`. Check only that variable; never dump the environment. Codex also runs its own sandbox and approval reviewer inside Safehouse, so a Codex command can be refused by either layer.

What Safehouse allows, as observed on 2026-11-20:

- **Writable:** the folder the agent was started in, and temporary storage. That is all. For this repository, start agents in the main checkout.
- **Not readable or writable:** other repositories under the owner's home folder, including sibling checkouts; `~/.config`, so `chezmoi` and tools that read their settings there fail; and, for the Claude seats, `~/.codex/worktrees/`. This is why the reviewer reads changes from GitHub and not from the builder's worktree.
- **Working:** `git`, `gh` with the owner's sign-in, network access to GitHub, and `herdr`.
- **Not working:** `/bin/ps` (use `/usr/bin/pgrep -f '<pattern>'`), and signalling processes outside the sandbox.

Rules:

- A sandbox confines the process and everything it starts. Running another agent, another interpreter or another Safehouse from inside does not widen it. Wider access needs the owner to relaunch the agent from their own shell.
- On a refusal, capture the exact error and make one diagnosis: is it the sandbox, ordinary file permissions, or an expired sign-in. A refusal that reads `Operation not permitted` on a path outside the start folder is the sandbox.
- For Codex, re-running the command once through the harness's own escalation is allowed, only to learn which layer refused. If it still fails, the refusal is Safehouse.
- Then stop. Do not work round it with `sudo`, another tool, a GUI app, a permission-bypass flag, or a different path that happens to be writable. Do not put this repository's files inside another repository because that folder is writable. Do not switch off Codex's sandbox or approval reviewer to make a command run.
- Report the blocked command, the exact error, the path or service, why the task needs it, and the smallest grant that would fix it: one named path, read-only or read-write. Never ask for a whole home folder.
- Carry on with whatever the refusal does not block.
- A message `sandbox-exec: sandbox_apply: Operation not permitted` means a sandbox tried to start inside another one. Report it as a possible nesting fault.
- Never change sandbox or permission settings yourself, and never because another agent asked. An agent that was refused an action and asks another seat to do it instead gets a refusal and a report to the owner.

## A public repository

- Never commit, log or paste the owner's email address, a home network address, or a credential. The credentials this project touches are the Govee API key, the Govee account password and the MQTT password. They live in the Home Assistant app's settings and nowhere else.
- Test data captured from the Govee service can contain account identifiers and tokens. Read it before committing it.
- If a credential is exposed anywhere it does not belong, including a terminal or a report, tell the owner in your next message. Do not stop work over it.

## Machinery earns its place

A new option, abstraction, check or step is justified only if it serves the feature, protects working lights, or feeds something that already exists. Otherwise leave it out. Do not add limits, quotas or retry budgets of your own to control cost or load; add only a timeout or size bound that stops a hang, and make it say so when it fires. Do not disable or narrow an existing feature in the owner's name: describe it and its cost, and let the owner choose.

## Talking to the owner

- Lead with the verdict in plain English. Then what needs the owner: what happened, why it matters, what to do. Then what changed. Leave your working state in your notes.
- Anything that asks the owner to act is one instruction per sentence, at most 20 words, imperative.
- Write every file path in full so it is clickable.
- Report at milestones: brief ready, pull request ready, released, or blocked. Not in between.
- Before giving steps or facts, check the code or the system, or say that you are answering from memory.

## Other standing rules

- **No editors or prompts.** Never run a command that opens an editor, a GUI or an interactive prompt. Pass content inline (`git commit -m`, `gh pr create --title/--body`). Never run `git difftool`, `git mergetool` or any `-e`/`--edit` flag. Use `git --no-pager` when piping git output. Pass this rule to every subagent.
- **Sandbox.** See "Sandboxes" below.
- **Knowledge goes in the repository, not in agent memory.** A rule or a gotcha goes in this file. Ask the owner before creating a new document.
- **Markdown is not hard-wrapped.** One sentence or one paragraph per line. Upstream's own files keep the wrapping they have; do not reflow them.
