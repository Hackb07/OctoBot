from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True)
class SandboxLimits:
    timeout_seconds: int = 120
    memory_mb: int = 1024
    cpu_count: float = 1.0
    network_enabled: bool = False
    writable_paths: tuple[str, ...] = (".",)
    dropped_capabilities: tuple[str, ...] = (
        "CAP_SYS_ADMIN",
        "CAP_NET_ADMIN",
        "CAP_SYS_PTRACE",
        "CAP_DAC_OVERRIDE",
    )


@dataclass
class RuntimePolicy:
    blocked_commands: set[str] = field(
        default_factory=lambda: {"sudo", "su", "mkfs", "shutdown", "reboot", "poweroff"}
    )
    approval_required_tools: set[str] = field(
        default_factory=lambda: {"write_file", "edit_file", "git_commit", "rollback_changes"}
    )
    default_limits: SandboxLimits = field(default_factory=SandboxLimits)

    def requires_approval(self, tool_name: str, dry_run: bool) -> bool:
        return not dry_run and tool_name in self.approval_required_tools
