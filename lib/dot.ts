export const dotCommands = ["apply", "doctor", "plan", "pull"] as const;

export type DotCommand = typeof dotCommands[number];

export type ParsedDotCommand =
  | { command?: DotCommand; type: "help" }
  | { command: DotCommand; type: "run" };

export class DotUsageError extends Error {}

const helpArguments = new Set(["-h", "--help", "help"]);

export function parseDotCommand(args: string[]): ParsedDotCommand {
  if (args.length === 0 || (args.length === 1 && helpArguments.has(args[0]!))) {
    return { type: "help" };
  }

  const [first, second] = args;
  if (first === "help") {
    if (args.length !== 2) {
      throw new DotUsageError("help expects exactly one command");
    }
    return { command: requireDotCommand(second!), type: "help" };
  }

  const command = requireDotCommand(first!);
  if (args.length === 1) {
    return { command, type: "run" };
  }
  if (args.length === 2 && helpArguments.has(second!)) {
    return { command, type: "help" };
  }
  throw new DotUsageError(`unexpected arguments for '${command}'`);
}

export function formatDotHelp(command?: DotCommand): string {
  if (command === undefined) {
    return [
      "Usage: dot <command>",
      "",
      "Commands:",
      "  pull    Fast-forward the local dotfiles checkout from origin/master",
      "  plan    Show planned home-directory deployment changes",
      "  apply   Review and apply home-directory deployment changes",
      "  doctor  Diagnose deployment, system, and package-manager drift",
      "",
      "Run 'dot help <command>' for details.",
    ].join("\n");
  }

  const descriptions: Record<DotCommand, string> = {
    apply: "Review and apply home-directory deployment changes.",
    doctor: "Diagnose deployment, mise, system, and Homebrew drift.",
    plan: "Show planned home-directory deployment changes without modifying files.",
    pull: "Fast-forward the clean local master branch from origin/master.",
  };
  return `Usage: dot ${command}\n\n${descriptions[command]}`;
}

function requireDotCommand(value: string): DotCommand {
  if ((dotCommands as readonly string[]).includes(value)) {
    return value as DotCommand;
  }
  throw new DotUsageError(`unknown command '${value}'`);
}
