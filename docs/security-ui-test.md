# Security UI Test Guide

Use this guide to verify the Phase 13 security dashboard in OctoBot.

## 1. Start OctoBot

From the project root, run:

```bash
cargo run
```

Expected result:

- The OctoBot terminal UI opens.
- The top bar shows workspace, health, role, and uptime.

## 2. Create a Blocked Security Event

Inside the OctoBot UI:

1. Press `/` to enter command mode.
2. Type:

```text
exec rm -rf /tmp/test
```

3. Press `Enter`.

Expected result:

- OctoBot blocks the command.
- The command should not execute.
- A failed or blocked execution record is created.

## 3. Open the Settings / Security View

After the blocked command:

1. If you are still in command mode, press `Esc`.
2. Press `9`.

Expected result:

- The Settings view opens.
- The security dashboard appears at the top.

## 4. Verify Security Dashboard Cards

In the Settings view, confirm that these cards are visible:

```text
Threats
Suspicious
Blocked
Violations
Vulns
Integrity
```

Expected result after running the blocked `rm -rf` command:

- `Blocked` should be `1` or more, or
- `Suspicious` should be `1` or more.

## 5. Verify Security Panels

Still in the Settings view, check for these panels:

- Command Security Audit
- Plugin Security Monitor
- Resource & Sandbox Protection
- Threat Timeline
- Vulnerability & Workflow Risk
- Security Replay & AI Trace

Expected result:

- The blocked command appears in the command audit or replay/security event area.
- The sandbox/resource panel shows current sandbox policy and role information.

## Troubleshooting

If pressing `9` does not switch views:

- Press `Esc` first to leave command mode.
- Then press `9` again.

If `Blocked` stays at `0`:

- Run the blocked command again:

```text
exec rm -rf /tmp/test
```

- Then return to Settings with `9`.

If the UI is hard to read:

- Resize the terminal to at least 120 columns wide.
