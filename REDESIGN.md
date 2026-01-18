# Installer Redesign

## Concept
Like using ChatGPT to install Arch Linux. User asks questions, LLM gives commands, user runs them.

## Layout
```
┌─────────────────────────────────┬─────────────────────────────────┐
│                                 │                                 │
│           CLI SHELL             │             CHAT                │
│                                 │                                 │
│  $ lsblk                        │  User: how do I partition?      │
│  NAME   SIZE                    │                                 │
│  sda    500G                    │  LLM: Run this command:         │
│                                 │  ```                            │
│  $ sgdisk -Z /dev/sda           │  sgdisk -Z /dev/sda && \        │
│  ...                            │  sgdisk -n 1:0:+512M ...        │
│                                 │  ```                            │
│  $ _                            │                                 │
│                                 │  User: ok done, what next?      │
│                                 │                                 │
└─────────────────────────────────┴─────────────────────────────────┘
         LEFT: Terminal                    RIGHT: Chat
```

## How it works
1. User boots into minimal shell environment
2. Split screen: real shell on left, chat on right
3. User asks "how do I install?" or "what's next?"
4. LLM responds with commands to copy/paste
5. User runs commands in the shell
6. User reports back "done" or asks for help

## No more
- ~~Checklist UI~~
- ~~Automatic command execution~~
- ~~Confirmation dialogs~~

## Key feature
- `?` command in shell opens/focuses chat
- Or just always visible split screen

## Simpler training data
- Input: user question + system state (lsblk output, etc.)
- Output: command to run + explanation
- No complex multi-turn state tracking needed

## TODO
- [ ] Implement split screen TUI (ratatui)
- [ ] Left pane: embedded shell (pty)
- [ ] Right pane: chat interface
- [ ] LLM integration for responses
- [ ] Training data for this simpler format
