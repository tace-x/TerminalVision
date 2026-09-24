# Architecture Rules

These rules apply to every change made to TerminalVision.

## Dependency direction

```
UI
↓
Application
↓
Domain / Filesystem
↓
Operating System
```

Dependencies point downwards only. The module map lives in `docs/ARCHITECTURE.md`.

## Rules

- Preserve the existing module architecture. It is a decision, not a draft.
- Do not move responsibilities between modules without a clear, stated reason.
- Keep UI separate from application state.
- Keep application logic separate from filesystem implementation.
- Filesystem code must not depend on UI code.
- Never introduce a circular dependency. A module may depend only on itself and on modules in the same or a lower layer.
- Do not create unnecessary abstractions. Add a trait, generic or wrapper only when a second real use exists.
- Do not create unnecessary files.
- Do not rewrite working code merely to implement a feature.
- Modify the smallest reasonable set of files for each task.
- Keep domain logic independently testable, without a terminal or a running UI.
