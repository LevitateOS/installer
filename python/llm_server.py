#!/usr/bin/env python3
"""
LLM Server for LevitateOS Installer

Extends the generic llm-toolkit server with:
- Real-time system fact gathering (disks, boot mode, mounts, etc.)
- Disk hallucination detection and blocking
- Installer-specific system prompt and tool schema

Usage:
    python llm_server.py --model vendor/models/SmolLM3-3B
    python llm_server.py --model vendor/models/SmolLM3-3B --adapter adapters/installer
"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

# Add llm-toolkit to path
SCRIPT_DIR = Path(__file__).parent.resolve()
TOOLKIT_DIR = SCRIPT_DIR.parent.parent / "llm-toolkit"
sys.path.insert(0, str(TOOLKIT_DIR))

from llm_server import LLMServer, run_server


# =============================================================================
# Installer-Specific Configuration
# =============================================================================

SHELL_COMMAND_TOOL = {
    "type": "function",
    "function": {
        "name": "run_shell_command",
        "description": "Execute a shell command for system installation tasks.",
        "parameters": {
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "The shell command to execute"}
            },
            "required": ["command"]
        }
    }
}

SYSTEM_PROMPT_TEMPLATE = """You are the LevitateOS installation assistant. Help users install their operating system.

{system_context}

CRITICAL RULES:
1. When user wants to DO something (list, format, partition, mount, create, set, install), ALWAYS call run_shell_command
2. When user CONFIRMS an action (yes, ok, proceed, continue, do it), EXECUTE the pending command via run_shell_command
3. When user asks a QUESTION (what is, how do, should I, explain), respond with text

COMMAND REFERENCE:
- List disks: lsblk
- Partition disk: sgdisk -Z /dev/X && sgdisk -n 1:0:+512M -t 1:ef00 -n 2:0:0 /dev/X
- Format EFI: mkfs.fat -F32 /dev/X1
- Format root: mkfs.ext4 /dev/X2
- Mount root: mount /dev/X2 /mnt
- Mount EFI: mkdir -p /mnt/boot/efi && mount /dev/X1 /mnt/boot/efi
- Set hostname: hostnamectl set-hostname NAME
- Set timezone: timedatectl set-timezone ZONE
- Create user: useradd -m -G wheel NAME
- Install GRUB: grub-install --target=x86_64-efi --efi-directory=/boot/efi

Only reference disks that exist in the system state above. Never hallucinate disk names."""


# =============================================================================
# System Facts Gathering
# =============================================================================

def gather_system_facts() -> dict:
    """Gather real system state - disks, mounts, users, etc."""
    facts = {}

    # Disks
    try:
        result = subprocess.run(
            ["lsblk", "-J", "-o", "NAME,SIZE,TYPE,MOUNTPOINT,FSTYPE,MODEL"],
            capture_output=True, text=True, timeout=5
        )
        if result.returncode == 0:
            facts["disks"] = json.loads(result.stdout)
    except Exception as e:
        facts["disks_error"] = str(e)

    # Boot mode (UEFI vs BIOS)
    facts["uefi"] = os.path.exists("/sys/firmware/efi/efivars")

    # Current mounts (only if /mnt exists and has something mounted)
    try:
        if os.path.ismount("/mnt"):
            result = subprocess.run(
                ["findmnt", "-J", "-R", "/mnt"],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0 and result.stdout.strip():
                facts["mounts"] = json.loads(result.stdout)
            else:
                facts["mounts"] = None
        else:
            facts["mounts"] = None
    except Exception:
        facts["mounts"] = None

    # Network connectivity
    try:
        result = subprocess.run(
            ["ping", "-c", "1", "-W", "2", "archlinux.org"],
            capture_output=True, timeout=5
        )
        facts["network"] = result.returncode == 0
    except Exception:
        facts["network"] = False

    # Current hostname
    try:
        facts["hostname"] = subprocess.run(
            ["hostname"], capture_output=True, text=True, timeout=2
        ).stdout.strip()
    except Exception:
        facts["hostname"] = "unknown"

    # Timezone
    try:
        tz_link = os.readlink("/etc/localtime")
        facts["timezone"] = tz_link.replace("/usr/share/zoneinfo/", "")
    except Exception:
        facts["timezone"] = "not set"

    # Existing users (non-system)
    try:
        users = []
        with open("/etc/passwd") as f:
            for line in f:
                parts = line.strip().split(":")
                if len(parts) >= 7:
                    uid = int(parts[2])
                    if 1000 <= uid < 60000:
                        users.append(parts[0])
        facts["users"] = users
    except Exception:
        facts["users"] = []

    return facts


def format_system_context(facts: dict) -> str:
    """Format system facts into a context string for the LLM."""
    lines = ["## Current System State\n"]

    # Boot mode
    if facts.get("uefi"):
        lines.append("- Boot mode: UEFI")
    else:
        lines.append("- Boot mode: Legacy BIOS")

    # Network
    if facts.get("network"):
        lines.append("- Network: Connected")
    else:
        lines.append("- Network: Not connected")

    # Hostname
    lines.append(f"- Hostname: {facts.get('hostname', 'unknown')}")

    # Timezone
    lines.append(f"- Timezone: {facts.get('timezone', 'not set')}")

    # Disks
    if "disks" in facts and "blockdevices" in facts["disks"]:
        lines.append("\n## Available Disks\n")
        for dev in facts["disks"]["blockdevices"]:
            if dev.get("type") == "disk":
                model = (dev.get("model") or "").strip() or "Unknown"
                lines.append(f"- /dev/{dev['name']}: {dev['size']} ({model})")
                if "children" in dev:
                    for part in dev["children"]:
                        mp = part.get("mountpoint", "")
                        fs = part.get("fstype", "")
                        mount_info = f" mounted at {mp}" if mp else ""
                        fs_info = f" [{fs}]" if fs else ""
                        lines.append(f"  - /dev/{part['name']}: {part['size']}{fs_info}{mount_info}")

    # Current mounts
    if facts.get("mounts"):
        lines.append("\n## Current Mounts\n")
        lines.append("Target partitions are mounted under /mnt")

    # Users
    if facts.get("users"):
        lines.append(f"\n## Existing Users: {', '.join(facts['users'])}")

    return "\n".join(lines)


# =============================================================================
# Installer-Specific LLM Server
# =============================================================================

class InstallerLLMServer(LLMServer):
    """
    LLM Server customized for LevitateOS installation.

    Adds:
    - System fact injection (disks, boot mode, etc.)
    - Disk hallucination detection
    """

    def __init__(self, model_path: str, adapter_path: str = None, **kwargs):
        # Override defaults for installer
        kwargs.setdefault("default_tools", [SHELL_COMMAND_TOOL])

        super().__init__(model_path, adapter_path, **kwargs)

        # Cache for valid disks
        self._valid_disks = set()
        self._cached_facts = None

    def gather_context(self) -> str:
        """Gather fresh system facts and cache valid disk names."""
        facts = gather_system_facts()
        self._cached_facts = facts

        # Build set of valid disk/partition names for verification
        self._valid_disks = set()
        if "disks" in facts and "blockdevices" in facts["disks"]:
            for dev in facts["disks"]["blockdevices"]:
                if dev.get("type") == "disk":
                    self._valid_disks.add(f"/dev/{dev['name']}")
                    if "children" in dev:
                        for part in dev["children"]:
                            self._valid_disks.add(f"/dev/{part['name']}")

        return format_system_context(facts)

    def build_system_prompt(self, base_prompt: str, context: str) -> str:
        """Build system prompt with context injection."""
        # Use our template which expects {system_context}
        return SYSTEM_PROMPT_TEMPLATE.format(system_context=context)

    def verify_response(self, result: dict) -> dict:
        """Verify that response doesn't reference non-existent disks/partitions."""
        # Handle tool_call type from toolkit's extract_response
        if result.get("type") == "tool_call":
            tool_name = result.get("tool_name", "")
            arguments = result.get("arguments", {})

            if tool_name == "run_shell_command":
                command = arguments.get("command", "")

                # Extract any /dev/* paths from the command
                dev_paths = re.findall(r'/dev/\w+', command)

                for path in dev_paths:
                    # Allow common pseudo-devices
                    if path in ["/dev/null", "/dev/zero", "/dev/urandom", "/dev/random"]:
                        continue

                    if path not in self._valid_disks:
                        # Hallucinated disk detected!
                        return {
                            "success": True,
                            "type": "text",
                            "response": f"I couldn't find {path} on this system. Let me check what disks are available.",
                            "warning": f"Blocked hallucinated disk: {path}",
                            "suggested_command": "lsblk"
                        }

                # Convert tool_call to installer's expected format
                return {
                    "success": True,
                    "type": "command",
                    "command": command,
                    "thinking": result.get("thinking"),
                }

        return result


def main():
    parser = argparse.ArgumentParser(description="LevitateOS Installer LLM Server")
    parser.add_argument("--model", "-m", default="vendor/models/SmolLM3-3B", help="Model path")
    parser.add_argument("--adapter", "-a", default=None, help="LoRA adapter path (optional)")
    parser.add_argument("--port", "-p", type=int, default=8765, help="Port to listen on")
    parser.add_argument("--host", default="127.0.0.1", help="Host to bind to")
    args = parser.parse_args()

    model_path = Path(args.model)
    if not model_path.is_absolute() and not model_path.exists():
        # Try relative to project root
        project_root = SCRIPT_DIR.parent.parent
        model_path = project_root / args.model
    model_path = model_path.resolve()

    if not model_path.exists():
        print(f"Error: Model not found at {model_path}", file=sys.stderr)
        sys.exit(1)

    server = InstallerLLMServer(
        str(model_path),
        adapter_path=args.adapter,
        dtype="float32",  # Installer uses float32 for compatibility
    )

    run_server(server, args.host, args.port)


if __name__ == "__main__":
    main()
