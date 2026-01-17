#!/usr/bin/env python3
"""
Train a LoRA adapter for the LevitateOS installer LLM.

This is a thin wrapper around llm-toolkit/train_lora.py with installer-specific defaults:
- Default data directory: installer/python/training
- Default output: installer/python/adapters/installer
- Default model: vendor/models/SmolLM3-3B
- Optimized hyperparameters for installer training

Usage:
    python train_lora.py  # Use all defaults
    python train_lora.py --model path/to/model --output adapters/my_adapter
    python train_lora.py --use-4bit  # Memory-efficient training
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent.resolve()
PROJECT_ROOT = SCRIPT_DIR.parent.parent
TOOLKIT_TRAINER = PROJECT_ROOT / "llm-toolkit" / "train_lora.py"

# Installer-specific defaults
DEFAULT_MODEL = PROJECT_ROOT / "vendor" / "models" / "SmolLM3-3B"
DEFAULT_DATA = SCRIPT_DIR / "training"
DEFAULT_OUTPUT = SCRIPT_DIR / "adapters" / "installer"
DEFAULT_TOOLS_JSON = SCRIPT_DIR / "tools.json"

# Installer-optimized hyperparameters
INSTALLER_DEFAULTS = {
    "epochs": 5,
    "lora_r": 32,
    "lora_alpha": 64,
    "max_length": 768,
    "eval_split": 0.1,
}


def ensure_tools_json():
    """Create tools.json if it doesn't exist."""
    if not DEFAULT_TOOLS_JSON.exists():
        import json
        tools = [{
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
        }]
        with open(DEFAULT_TOOLS_JSON, "w") as f:
            json.dump(tools, f, indent=2)
        print(f"Created {DEFAULT_TOOLS_JSON}")


def main():
    parser = argparse.ArgumentParser(
        description="Train LoRA adapter for LevitateOS installer",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
    python train_lora.py                      # Use all defaults
    python train_lora.py --use-4bit           # Memory-efficient
    python train_lora.py --epochs 10          # More epochs
    python train_lora.py --output adapters/v2 # Custom output
        """
    )

    # Paths
    parser.add_argument("--model", "-m", default=str(DEFAULT_MODEL),
                        help=f"Base model path (default: {DEFAULT_MODEL.relative_to(PROJECT_ROOT)})")
    parser.add_argument("--data", "-d", default=str(DEFAULT_DATA),
                        help=f"Training data path (default: {DEFAULT_DATA.relative_to(PROJECT_ROOT)})")
    parser.add_argument("--output", "-o", default=str(DEFAULT_OUTPUT),
                        help=f"Output directory (default: {DEFAULT_OUTPUT.relative_to(PROJECT_ROOT)})")

    # Training hyperparameters (installer defaults)
    parser.add_argument("--epochs", type=int, default=INSTALLER_DEFAULTS["epochs"],
                        help=f"Training epochs (default: {INSTALLER_DEFAULTS['epochs']})")
    parser.add_argument("--lora-r", type=int, default=INSTALLER_DEFAULTS["lora_r"],
                        help=f"LoRA rank (default: {INSTALLER_DEFAULTS['lora_r']})")
    parser.add_argument("--lora-alpha", type=int, default=INSTALLER_DEFAULTS["lora_alpha"],
                        help=f"LoRA alpha (default: {INSTALLER_DEFAULTS['lora_alpha']})")
    parser.add_argument("--max-length", type=int, default=INSTALLER_DEFAULTS["max_length"],
                        help=f"Max sequence length (default: {INSTALLER_DEFAULTS['max_length']})")
    parser.add_argument("--batch-size", type=int, default=1,
                        help="Batch size (default: 1)")
    parser.add_argument("--learning-rate", type=float, default=1e-4,
                        help="Learning rate (default: 1e-4)")
    parser.add_argument("--eval-split", type=float, default=INSTALLER_DEFAULTS["eval_split"],
                        help=f"Eval split ratio (default: {INSTALLER_DEFAULTS['eval_split']})")

    # Memory optimization
    parser.add_argument("--use-4bit", action="store_true",
                        help="Use 4-bit quantization (saves memory)")
    parser.add_argument("--use-8bit", action="store_true",
                        help="Use 8-bit quantization (saves memory)")
    parser.add_argument("--cpu", action="store_true",
                        help="Force CPU training (slow)")
    parser.add_argument("--no-gradient-checkpointing", action="store_true",
                        help="Disable gradient checkpointing")

    args = parser.parse_args()

    # Ensure tools.json exists
    ensure_tools_json()

    # Build command for toolkit trainer
    cmd = [
        sys.executable, str(TOOLKIT_TRAINER),
        "--model", args.model,
        "--data", args.data,
        "--output", args.output,
        "--epochs", str(args.epochs),
        "--lora-r", str(args.lora_r),
        "--lora-alpha", str(args.lora_alpha),
        "--max-length", str(args.max_length),
        "--batch-size", str(args.batch_size),
        "--learning-rate", str(args.learning_rate),
        "--eval-split", str(args.eval_split),
        "--tools-json", str(DEFAULT_TOOLS_JSON),
    ]

    if args.use_4bit:
        cmd.append("--use-4bit")
    if args.use_8bit:
        cmd.append("--use-8bit")
    if args.cpu:
        cmd.append("--cpu")
    if args.no_gradient_checkpointing:
        cmd.append("--no-gradient-checkpointing")

    print("=" * 60)
    print("LevitateOS Installer LoRA Training")
    print("=" * 60)
    print(f"Model: {args.model}")
    print(f"Data:  {args.data}")
    print(f"Output: {args.output}")
    print(f"Epochs: {args.epochs}, LoRA r={args.lora_r}, alpha={args.lora_alpha}")
    print("=" * 60)
    print()

    # Run the toolkit trainer
    os.chdir(PROJECT_ROOT)  # Ensure relative paths work
    result = subprocess.run(cmd)
    sys.exit(result.returncode)


if __name__ == "__main__":
    main()
