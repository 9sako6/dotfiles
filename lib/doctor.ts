export type MiseInventory = {
  missing: string[];
  prunable: string[];
};

export type HomebrewInventory = {
  missing: string[];
  unmanaged: string[];
};

export type DoctorSectionContent = {
  findings: string[];
  nextSteps: string[];
  summary: string;
};

export type DoctorInspection = {
  inspect: () => Promise<DoctorSectionContent>;
  title: string;
};

type MiseRecord = {
  installed?: boolean;
  version?: string;
};

export function inspectMise(options: {
  missingRaw: string;
  prunableRaw: string;
}): MiseInventory {
  return {
    missing: miseVersions(options.missingRaw, false),
    prunable: miseVersions(options.prunableRaw, true),
  };
}

function miseVersions(raw: string, installed: boolean): string[] {
  const parsed = JSON.parse(raw) as Record<string, MiseRecord[]>;
  const entries: string[] = [];
  for (const [tool, versions] of Object.entries(parsed)) {
    for (const version of versions) {
      if (version.installed !== installed || typeof version.version !== "string") {
        continue;
      }
      entries.push(`${tool}@${version.version}`);
    }
  }
  return entries.sort();
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

function formatDoctorSection(
  title: string,
  summary: string,
  findings: readonly string[],
  nextSteps: readonly string[],
): string {
  const lines = [`[${title}]`, `  ${summary}`];
  for (const finding of findings) {
    lines.push(`  warning: ${finding}`);
  }
  for (const nextStep of nextSteps) {
    lines.push(`  hint: ${nextStep}`);
  }
  return lines.join("\n");
}

export async function runDoctor(inspections: readonly DoctorInspection[]): Promise<{
  failed: boolean;
  output: string;
}> {
  const sections = await Promise.all(
    inspections.map(async ({ inspect, title }) => {
      try {
        return {
          content: await inspect(),
          failed: false as const,
          title,
        };
      } catch (error) {
        return {
          error: error instanceof Error ? error.message : String(error),
          failed: true as const,
          title,
        };
      }
    }),
  );
  return {
    failed: sections.some(
      (section) => section.failed || section.content.findings.length > 0,
    ),
    output: sections.map((section) => {
      if (section.failed) {
        return [`[${section.title}]`, "  diagnosis failed", `  error: ${section.error}`].join("\n");
      }
      return formatDoctorSection(
        section.title,
        section.content.summary,
        section.content.findings,
        section.content.nextSteps,
      );
    }).join("\n\n"),
  };
}
