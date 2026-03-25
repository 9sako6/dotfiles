import path from "node:path";

function pad(value: number) {
  return String(value).padStart(2, "0");
}

export function createTimestamp(date = new Date()) {
  return [
    date.getFullYear(),
    pad(date.getMonth() + 1),
    pad(date.getDate()),
    "T",
    pad(date.getHours()),
    pad(date.getMinutes()),
    pad(date.getSeconds()),
  ].join("");
}

export function backupRootFor(homeDir: string, timestamp: string) {
  return path.join(homeDir, ".dotfiles-backups", timestamp);
}

export function backupPathFor(homeDir: string, destinationPath: string, timestamp: string) {
  return path.join(backupRootFor(homeDir, timestamp), path.relative(homeDir, destinationPath));
}
