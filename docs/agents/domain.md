# Domain Docs

This repo uses a single-context layout.

## Before exploring

- Read `GLOSSARY.md` at the repo root.
- Read ADRs in `docs/adr/` that touch the area being explored.

If these files or directories do not exist, proceed silently.
The `/domain-modeling` skill creates them lazily when terms or
decisions are resolved.

## File structure

```text
/
├── GLOSSARY.md
└── docs/
    └── adr/
        └── NNNN-<decision-slug>.md
```

## Use the glossary's vocabulary

Use the terms defined in `GLOSSARY.md` when naming domain concepts
in issues, proposals, hypotheses, and tests. Follow its Avoid guidance.

When a needed concept is absent, reconsider the wording or note
the gap for `/domain-modeling`.

## Flag ADR conflicts

If a proposal contradicts an existing ADR, identify the ADR
explicitly and explain why the decision should be reopened.
