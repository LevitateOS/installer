# Installer Development Requirements

## HARD REQUIREMENTS

### Fedora Version: 43

**The dev VM and live ISO MUST use Fedora 43.**

- Dev VM cloud image: Fedora 43 Cloud Base
- Live ISO kickstart: `--releasever 43`

This ensures the dev environment matches production exactly.

### Why Fedora 43?

1. LevitateOS targets Fedora 43 as the base
2. Package versions must match between dev and production
3. systemd, kernel, and other core components must be consistent
4. Testing on older Fedora versions can hide bugs

### Updating Fedora Version

If Fedora version needs to change in the future:

1. Update `FEDORA_VERSION` in `xtask/src/vm.rs`
2. Update `FEDORA_IMAGE_URL` in `xtask/src/vm.rs`
3. Update `--releasever` in `kickstarts/levitate-live.ks`
4. Run `cargo xtask vm setup --force` to re-download
5. Rebuild ISO with `cargo xtask vm iso-build --force`

## Other Requirements

### Dev VM

- QEMU with KVM support
- `genisoimage` or `mkisofs`
- `curl`

### ISO Building

- `lorax` / `livemedia-creator` (Fedora only)
- Root access (sudo)
