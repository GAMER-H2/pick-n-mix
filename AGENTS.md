# Agent Working Agreement

## Goal
Make correct and maintainable changes that fit the existing architecture.
Prefer understanding the repository over guessing.

## Before editing
For tasks beyond a small isolated fix:

1. Inspect the relevant implementation, types, tests, configuration, and call sites.
2. State a short plan: files likely to change, approach, risks, and verification steps.
3. If the requested behaviour or trade-off is materially ambiguous, ask a concise
   question before making irreversible architectural choices.
4. Reuse existing project patterns before introducing new abstractions or dependencies.

For clearly small, unambiguous changes, proceed directly and explain the result.

## Implementation
- Preserve public APIs and existing behaviour unless the task explicitly changes them.
- Use strict typing; do not use `any`, unsafe casts, or suppressed errors without
  explaining why they are necessary.
- Match the formatting, naming, error handling, and module organisation already used.
- Do not alter generated files, lockfiles, migrations, production configuration, or
  dependencies unless required. Explain such changes clearly.

## Tests and verification
- Add or update tests for every new behaviour and bug fix when practical.
- Cover normal operation, invalid input/error paths, and important regressions.
- Run the narrowest relevant checks first, then the project checks appropriate to
  the change.
- Never say a change is verified unless the relevant command actually succeeded.
- If checks cannot be run because of sandbox, missing tooling, a GUI limitation, or
  time, state exactly:
  - which check was not run
  - why it could not be run
  - the exact command or manual procedure to run locally
- Do not weaken, skip, or delete tests merely to make a suite pass.

## GUI and visual work
- For UI changes, inspect the existing UI implementation and related styles first.
- Run available lint, type-check, unit, and end-to-end checks.
- If screenshots or GUI automation are available, use them to validate visual changes.
- If they are not available, do not claim visual confirmation. Describe the exact
  local steps and viewport/state needed to verify it.

## Completion report
End every task with:
1. Summary of behaviour changed
2. Files changed and why
3. Tests/checks run and their results
4. Tests/checks not run and why
5. Any follow-up risks, assumptions, or manual verification needed
