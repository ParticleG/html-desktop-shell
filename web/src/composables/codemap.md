# `web/src/composables/`

## Responsibility

`useShellPolling.ts` owns view-scoped, serialized shell-state polling. It is used by the Bar, Quick Panel, and Diagnostics views; the Workspace HUD intentionally never calls it.

## Lifecycle and scheduling

- `POLL_INTERVAL_MS` is 1000 ms.
- `useShellPolling()` obtains the shared `useShellStore()` and registers Vue mount/unmount hooks.
- On mount it starts `poll()` immediately. Each `poll()` awaits `shell.refreshState()` before scheduling the next run with `window.setTimeout`, so the cadence never overlaps requests and does not accumulate interval callbacks.
- The store's `isRefreshing` guard also serializes calls if another consumer or action requests a refresh concurrently.
- On unmount, `disposed` is set before `clearTimeout(timer)`. A refresh that settles after disposal observes the latch and does not schedule another timeout.
- The composable performs no HostInfo or capability bootstrap; `SurfaceRoot.vue` owns that one-time initialization before a surface view is dispatched.

## Consumers and boundaries

- `BarView.vue`, `QuickPanelView.vue`, and `DiagnosticsDialogView.vue` each own polling for their mounted surface and stop it with their lifecycle.
- `WorkspaceHudView.vue` renders the compositor-confirmed `WorkspaceHudSurfaceContext` passed by `SurfaceRoot`; it has immutable display data and no state polling.
- `shell.ts` owns state, bridge errors, and refresh concurrency. This composable owns only lifecycle and scheduling.
- Mounted Bar tests cover an in-flight refresh at cleanup, continued serialized polling while mounted, and timer removal after unmount.
