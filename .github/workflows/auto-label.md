---
on:
  issues:
    types: [opened, edited]
  pull_request:
    types: [opened, edited, synchronize]
permissions:
  contents: read
  issues: write
  pull-requests: write
safe-outputs:
  add-labels:
---

# Auto-Labeler

Automatically label newly opened or edited issues and pull requests based on
their content.

## Instructions

1. Read the title and body of the issue or pull request.
2. Fetch the full list of labels available in this repository.
3. Choose the most relevant labels (1–3) from that list that describe the
   item. Do **not** create new labels; only use existing ones.
4. Apply the selected labels to the issue or pull request.

## Labeling Guidelines

- `bug` — the item describes a defect, crash, panic, or incorrect behavior
- `enhancement` — the item requests a new feature or improvement
- `documentation` — the item relates to docs, README, examples, or guides
- `question` — the item asks for help, clarification, or discussion
- `good first issue` — the item is well-scoped and suitable for a newcomer
- `dependencies` — the item relates to cargo, lock-file, or dependency updates
- `security` — the item concerns a vulnerability or supply-chain risk
- `performance` — the item concerns speed, memory usage, or efficiency
- `refactoring` — the item cleans up code without changing functionality
- For PRs: prefer `bug` when fixing a defect, `enhancement` when adding a
  feature, and `documentation` when only changing docs or comments.

Apply the most specific matching label(s). When nothing fits, leave the item
unlabeled rather than guessing.
