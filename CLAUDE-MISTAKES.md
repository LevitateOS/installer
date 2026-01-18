# Claude Mistakes to NEVER Repeat

## FIXES ARE NOT FLAGS

**When something is broken and needs fixing, the fix becomes THE DEFAULT.**

### The Anti-Pattern (DO NOT DO THIS)

```
"I fixed the login prompt issue. Run with --skip-login to use the fix."
"I updated to Fedora 43. Use --fedora-43 flag to enable it."
"The TUI now starts automatically. Pass --with-tui to enable."
```

This results in command lines like:
```bash
cargo xtask vm start --skip-login --fedora-43 --with-tui --increased-cpu
```

**This is WRONG.** Each "fix" becomes technical debt disguised as flexibility.

### The Correct Approach

When you fix something:
1. **Replace the broken code entirely**
2. **Make the fix the new default**
3. **Delete the old broken behavior**
4. **Do NOT preserve old behavior behind flags**

The only flags should be for GENUINE user choices (like `--gui` vs headless), NOT for "enable the thing that actually works."

### Why This Matters

- Flags accumulate over time into unusable command lines
- New users don't know which flags are "required" vs optional
- It implies the fix is experimental when it should be the standard
- It preserves broken code paths that should be deleted

### Rule

**If I asked you to fix something, that fix is now THE DEFAULT. The old broken code gets DELETED, not hidden behind a flag.**
