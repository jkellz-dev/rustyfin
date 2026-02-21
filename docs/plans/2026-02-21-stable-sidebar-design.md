# Stable Sidebar Layout — Design

**Date:** 2026-02-21
**Problem:** Navigating between admin pages causes the sidebar to flicker/rebuild because the entire view tree (sidebar + content) lives inside a reactive `(match route.get_clone() ...)` block.

## Root Cause

Sycamore's `Router` calls its `view` function once with a `ReadSignal<AppRoutes>`. Inside `view!`, any `(match signal.get_clone() { ... })` creates a dynamic DOM node that replaces its **entire subtree** when the signal changes. Currently both the sidebar and page content are inside this dynamic block, so both get torn down and rebuilt on every navigation.

## Solution: Isolate the reactive dependency

Structure the DOM so the sidebar is a sibling of — not inside — the reactive match block. Sycamore only rebuilds dynamic nodes that read a signal; static siblings are untouched.

### Architecture

```
Router(view = FnOnce(ReadSignal<AppRoutes>) -> View)
└── AppShell (created once)
    ├── Sidebar {}                          ← no signal reads, never rebuilt
    └── main.main-content
        └── (match route.get_clone() { ... }) ← reactive, only this swaps
```

### Files

- **`main.rs`** — Router's view closure renders `AppShell` with route signal as prop. Setup page handled as special case before the shell.
- **`components/nav.rs`** — `Sidebar` is a static component (no children/wrapper). Remove old `Layout` component. Add `AppShell` component that combines sidebar + page switcher.

### Key Sycamore behavior

In a `view!` block, sibling nodes are independent. Only nodes that read a signal are reactive. So `Sidebar {}` next to a `(match route.get() { ... })` stays stable while the match output swaps.

### Click handling

The Router's built-in click handler (attached to root DOM nodes) intercepts `<a>` clicks for SPA navigation. Using `navigate()` in our own `on:click` handlers causes a dual-update conflict — both our handler and the Router's handler fire. Fix: call `stop_propagation()` on nav link clicks so only our handler runs. This gives consistent single-update behavior on every click.
