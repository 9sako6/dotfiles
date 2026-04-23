---
name: logging-bash-commands
description: Internal hook-only skill for logging Claude Code Bash commands. Do not invoke directly.
license: Apache-2.0
metadata:
  author: 9sako6
  version: "1.0.0"
user-invocable: false
hooks:
  PostToolUse:
    - matcher: Bash
      hooks:
        - type: command
          command: "jq -r '.tool_input.command' >> ~/.claude/command-log.txt"
---

# Logging Bash Commands

Internal hook-only skill. Do not invoke directly.
