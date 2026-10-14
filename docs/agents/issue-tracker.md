# Issue tracker: Local Markdown

Issues and specs for this repo live as Markdown files in `.scratch/`.

## Conventions

- One feature per directory: `.scratch/<feature-slug>/`
- Specs: `.scratch/<feature-slug>/spec.md`
- Implementation issues: `.scratch/<feature-slug>/issues/<NN>-<slug>.md`,
  numbered from `01`, one file per ticket.
- Triage state: a `Status:` line near the top of each issue file.
  Use the role strings from `triage-labels.md`.
- Append comments and conversation history under `## Comments`.

## Publish to the issue tracker

Create the spec or individual issue files at the paths above,
creating directories as needed.

## Fetch the relevant ticket

Read the referenced issue file. The user normally supplies its path
or issue number; resolve numbers within the relevant feature directory.

## Wayfinding operations

Used by `/wayfinder`. The map has one child file per ticket.

- Map: `.scratch/<effort>/map.md`, containing Notes,
  Decisions-so-far, and Fog.
- Child ticket: `.scratch/<effort>/issues/NN-<slug>.md`,
  numbered from `01`, with the question in the body.
- `Type:` records `research`, `prototype`, `grilling`, or `task`.
- Wayfinding `Status:` records `open`, `claimed`, or `resolved`.
- `Blocked by: NN, NN` lists dependencies. A ticket is unblocked
  when every listed ticket is `resolved`.
- Frontier: scan for open, unblocked, unclaimed tickets;
  the lowest number wins.
- Claim: set `Status: claimed` and save before starting work.
- Resolve: append the answer under `## Answer`, set
  `Status: resolved`, and append a gist and link to the map's
  Decisions-so-far.
