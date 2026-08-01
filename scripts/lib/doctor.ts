export type MiseInventory = {
  prunable: string[];
};

export type HomebrewInventory = {
  missing: string[];
  unmanaged: string[];
};

type MiseRecord = {
  installed?: boolean;
  version?: string;
};

export function inspectMise(raw: string): MiseInventory {
  const parsed = JSON.parse(raw) as Record<string, MiseRecord[]>;
  const prunable: string[] = [];
  for (const [tool, versions] of Object.entries(parsed)) {
    for (const version of versions) {
      if (!version.installed || typeof version.version !== "string") {
        continue;
      }
      prunable.push(`${tool}@${version.version}`);
    }
  }
  return { prunable: prunable.sort() };
}

export function inspectHomebrew(options: {
  declaredCasks: ReadonlySet<string>;
  declaredFormulae: ReadonlySet<string>;
  installedCasks: readonly string[];
  installedFormulae: readonly string[];
}): HomebrewInventory {
  const installedCasks = new Set(options.installedCasks);
  const installedFormulae = new Set(options.installedFormulae);
  const missing = [
    ...[...options.declaredCasks]
      .filter((name) => !installedCasks.has(name))
      .map((name) => `brew-cask:${name}`),
    ...[...options.declaredFormulae]
      .filter((name) => !installedFormulae.has(name))
      .map((name) => `brew:${name}`),
  ].sort();
  const unmanaged = [
    ...options.installedCasks
      .filter((name) => !options.declaredCasks.has(name))
      .map((name) => `brew-cask:${name}`),
    ...options.installedFormulae
      .filter((name) => !options.declaredFormulae.has(name))
      .map((name) => `brew:${name}`),
  ].sort();
  return { missing, unmanaged };
}

export function formatDoctorSection(
  title: string,
  summary: string,
  findings: readonly string[],
): string {
  const lines = [`[${title}]`, `  ${summary}`];
  for (const finding of findings) {
    lines.push(`  warning: ${finding}`);
  }
  return lines.join("\n");
}
