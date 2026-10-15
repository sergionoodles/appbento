import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const RELEASE_FILES = [
  "package.json",
  "src-tauri/tauri.conf.json",
  "src-tauri/Cargo.toml",
  "Cargo.lock",
];
const STABLE_VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

export function bumpVersion(version: string, increment: string): string {
  if (!["patch", "minor", "major"].includes(increment)) {
    throw new Error("Usage: pnpm release patch|minor|major");
  }
  if (!STABLE_VERSION.test(version)) {
    throw new Error(`Expected a stable major.minor.patch version, received ${version}.`);
  }

  const [major, minor, patch] = version.split(".").map(Number);
  if (increment === "major") return `${major + 1}.0.0`;
  if (increment === "minor") return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

function run(command: string, arguments_: string[], cwd: string): string {
  return execFileSync(command, arguments_, { cwd, encoding: "utf8" }).trim();
}

function runCheck(command: string, arguments_: string[], cwd: string): void {
  execFileSync(command, arguments_, { cwd, stdio: "inherit" });
}

function readVersionFiles(cwd: string) {
  const files = RELEASE_FILES.slice(0, 3).map(path => ({
    path,
    content: readFileSync(resolve(cwd, path), "utf8"),
  }));
  const versions = files.map(({ path, content }) => {
    if (path.endsWith(".json")) {
      const data: unknown = JSON.parse(content);
      if (typeof data === "object" && data !== null && "version" in data && typeof data.version === "string") {
        return data.version;
      }
      throw new Error(`Missing version in ${path}.`);
    }
    const packageSection = content.split(/(?=^\[)/m).find(section => section.startsWith("[package]"));
    const version = packageSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
    if (!version) throw new Error(`Missing package version in ${path}.`);
    return version;
  });
  const version = versions[0];
  if (versions.some(candidate => candidate !== version)) {
    throw new Error("All three app versions must match before releasing.");
  }
  return { files, version };
}

export function release(increment: string, cwd: string): string {
  const { files, version } = readVersionFiles(cwd);
  const nextVersion = bumpVersion(version, increment);
  const tag = `v${nextVersion}`;
  if (run("git", ["status", "--porcelain", "--untracked-files=all"], cwd)) {
    throw new Error("Commit or stash all changes before releasing.");
  }

  const branch = run("git", ["symbolic-ref", "--quiet", "--short", "HEAD"], cwd);
  const remote = run("git", ["config", "--get", `branch.${branch}.remote`], cwd);
  const destination = run("git", ["config", "--get", `branch.${branch}.merge`], cwd);
  if (remote === "." || !destination.startsWith("refs/heads/")) {
    throw new Error("The current branch must track a branch on a remote.");
  }
  runCheck("git", ["fetch", "--tags", remote], cwd);
  const behind = run("git", ["rev-list", "--count", "HEAD..@{upstream}"], cwd);
  if (behind !== "0") throw new Error("Update your branch from its upstream before releasing.");
  const tags = run("git", ["tag", "--list", tag], cwd);
  if (tags) throw new Error(`Release tag ${tag} already exists.`);

  for (const { path, content } of files) {
    const pattern = path.endsWith(".json")
      ? /("version"\s*:\s*")[^"]+(")/
      : /(^\[package\]\s*\n[\s\S]*?^version\s*=\s*")[^"]+(")/m;
    const updated = content.replace(
      pattern,
      (_match, prefix: string, suffix: string) => `${prefix}${nextVersion}${suffix}`,
    );
    writeFileSync(resolve(cwd, path), updated);
  }

  runCheck("cargo", ["update", "--workspace", "--offline"], cwd);
  runCheck("pnpm", ["check"], cwd);
  runCheck("cargo", ["test", "--locked", "--workspace"], cwd);

  const changedFiles = run("git", ["diff", "--name-only", "HEAD"], cwd).split("\n");
  if (changedFiles.some(path => !RELEASE_FILES.includes(path)) || run("git", ["ls-files", "--others", "--exclude-standard"], cwd)) {
    throw new Error("Checks changed files outside the release versions. Inspect the working tree before continuing.");
  }
  runCheck("git", ["add", "--", ...RELEASE_FILES], cwd);
  runCheck("git", ["commit", "-m", `chore: release ${tag}`], cwd);
  runCheck("git", ["tag", tag], cwd);
  runCheck("git", ["push", "--atomic", remote, `HEAD:${destination}`, `refs/tags/${tag}`], cwd);
  return tag;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length !== 3) throw new Error("Usage: pnpm release patch|minor|major");
    const tag = release(process.argv[2], process.cwd());
    process.stdout.write(`Pushed ${tag}. GitHub Actions will build the AppImage and create a draft release.\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\nInspect git status and local tags before retrying; completed local steps are preserved.\n`);
    process.exitCode = 1;
  }
}
