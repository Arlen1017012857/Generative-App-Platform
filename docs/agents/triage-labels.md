# Triage Labels

Map canonical triage roles to the strings used by this repo's tracker.

| Canonical role | Tracker string | Meaning |
| --- | --- | --- |
| `needs-triage` | `needs-triage` | Maintainer needs to evaluate the issue |
| `needs-info` | `needs-info` | Waiting on the reporter for more information |
| `ready-for-agent` | `ready-for-agent` | Fully specified, ready for an AFK agent |
| `ready-for-human` | `ready-for-human` | Requires human implementation |
| `wontfix` | `wontfix` | Will not be actioned |

When a skill mentions a triage role, use its tracker string in the
issue file's `Status:` line.

Edit the tracker-string column to change the vocabulary.
