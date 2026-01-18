# LevitateOS Installer

A tmux-based installer with a split view: native bash shell (left) for running commands, and a TypeScript TUI docs viewer (right) for guidance.

## Two Testing Workflows

| Workflow | Speed | Accuracy | Use For |
|----------|-------|----------|---------|
| **Dev VM** | Fast (seconds) | Good | Quick iteration on UI/docs |
| **ISO Test** | Slow (minutes to build) | Exact | Final validation before release |

## Workflow 1: Dev VM (Fast Iteration)

Uses a Fedora cloud image for quick edit-sync-test cycles. Good for UI and documentation development.

### Prerequisites

```bash
# Fedora/RHEL
sudo dnf install qemu-kvm genisoimage

# Ubuntu/Debian
sudo apt install qemu-kvm genisoimage

# Arch
sudo pacman -S qemu-full cdrtools
```

### First-Time Setup (One-Time)

```bash
cd installer
cargo xtask vm setup    # Downloads ~500MB Fedora cloud image
```

### Iterative Development

```bash
# Start VM with GUI (recommended)
cargo xtask vm start --gui

# In another terminal, sync your code changes
cargo xtask vm sync

# In the VM window, run:
levitate-installer
```

**Edit → Sync → Test loop:**
```bash
# 1. Edit code on host
vim src/components/DocsPanel.tsx

# 2. Sync to VM (from host terminal)
cargo xtask vm sync

# 3. In VM window, run levitate-installer again
```

### Dev VM Commands

| Command | Description |
|---------|-------------|
| `cargo xtask vm setup` | First-time setup (downloads ~500MB) |
| `cargo xtask vm start --gui` | Start VM with graphical display |
| `cargo xtask vm start --detach` | Start VM in background (SSH-only) |
| `cargo xtask vm stop` | Stop the VM |
| `cargo xtask vm sync` | Sync code to VM + install deps |
| `cargo xtask vm status` | Check VM state |
| `cargo xtask vm ssh` | SSH into the VM |
| `cargo xtask vm reset` | Reset disk to clean state |

## Workflow 2: ISO Test (Accurate Validation)

Builds and boots the actual LevitateOS live ISO. Use this to validate that everything works exactly as it will in production.

### Prerequisites

```bash
# Fedora only (livemedia-creator)
sudo dnf install lorax
```

### Build the ISO

```bash
cargo xtask vm iso-build    # Takes several minutes, requires sudo
```

### Run the ISO

```bash
cargo xtask vm iso-run
```

In the VM:
1. Login as `root` (no password)
2. Wait for first-boot setup (installs bun)
3. Run: `levitate-installer`

### ISO Test Commands

| Command | Description |
|---------|-------------|
| `cargo xtask vm iso-build` | Build LevitateOS ISO (~10-15 min) |
| `cargo xtask vm iso-build --force` | Force rebuild |
| `cargo xtask vm iso-run` | Boot the ISO in QEMU |

## Key Differences: Dev VM vs ISO

| Aspect | Dev VM | Live ISO |
|--------|--------|----------|
| User | `dev` | `root` |
| Bun location | `~/.bun/bin/bun` | `/usr/local/bin/bun` |
| Installer | `~/installer/bin/` | `/mnt/share/installer/bin/` |
| File sync | `cargo xtask vm sync` | Automatic via virtfs |
| Boot time | ~30s | ~1 min |
| Persistence | Yes (disk image) | No (RAM-based) |

## Troubleshooting

### Dev VM Issues

**"SSH not available after 90s"**
```bash
tail -f .vm/serial.log
```

**"/dev/kvm not accessible"**
```bash
sudo usermod -aG kvm $USER
# Log out and back in
```

**Reset everything**
```bash
cargo xtask vm stop
cargo xtask vm reset
```

### ISO Issues

**"livemedia-creator not found"**
```bash
sudo dnf install lorax
```

**Build failed**
```bash
# Check logs
cat kickstarts/build/livemedia.log
```

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│ Host Machine                                                    │
│                                                                 │
│  installer/                                                     │
│  ├── src/                  ← TypeScript source                  │
│  ├── bin/levitate-installer← tmux launcher                      │
│  └── xtask/                ← VM management CLI                  │
│                                                                 │
│  kickstarts/                                                    │
│  └── levitate-live.ks      ← ISO build configuration            │
│                                                                 │
│         │                                                       │
│         ▼                                                       │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Dev VM (Fedora)          OR    Live ISO (LevitateOS)    │   │
│  │                                                         │   │
│  │  /mnt/share/ ← Host project via virtfs                  │   │
│  │                                                         │   │
│  │  levitate-installer ← Runs the tmux split view          │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
```

## Files

| File | Purpose |
|------|---------|
| `src/index.tsx` | Docs viewer entry point |
| `src/components/DocsPanel.tsx` | Documentation renderer |
| `bin/levitate-installer` | tmux launcher script |
| `xtask/src/vm.rs` | VM management implementation |
| `.vm/` | Dev VM disk images (gitignored) |
| `../kickstarts/levitate-live.ks` | ISO build configuration |
