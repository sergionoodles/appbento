// @vitest-environment node
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, test } from "vitest";
import { bumpVersion } from "../scripts/release.ts";

const releaseScript = fileURLToPath(new URL("../scripts/release.ts", import.meta.url));
const temporaryDirectories: string[] = [];
const INITIAL_VERSION = "0.6.5";
const NEXT_TAG = "v0.6.6";

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

function createRepository() {
  const directory = mkdtempSync(join(tmpdir(), "appbento-release-"));
  temporaryDirectories.push(directory);
  const repository = join(directory, "repository");
  const remote = join(directory, "remote.git");
  const bin = join(directory, "bin");
  const env = {
    ...process.env,
    PATH: `${bin}:${process.env.PATH ?? ""}`,
    GIT_CONFIG_GLOBAL: "/dev/null",
    GIT_CONFIG_NOSYSTEM: "1",
    CARGO_TARGET_DIR: join(directory, "target"),
  };
  mkdirSync(join(repository, "src-tauri", "src"), { recursive: true });
  mkdirSync(bin);
  const git = (...arguments_: string[]) => execFileSync("git", arguments_, {
    cwd: repository, env, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
  }).trim();
  const write = (path: string, content: string) => writeFileSync(join(repository, path), content);
  write(".gitignore", "target/\n");
  write("package.json", JSON.stringify({ name: "fixture", version: INITIAL_VERSION }));
  write("src-tauri/tauri.conf.json", JSON.stringify({ version: INITIAL_VERSION }));
  write("Cargo.toml", '[workspace]\nmembers = ["src-tauri"]\nresolver = "2"\n');
  write("src-tauri/Cargo.toml", `[package]\nname = "fixture"\nversion = "${INITIAL_VERSION}"\nedition = "2021"\n`);
  write("src-tauri/src/main.rs", "fn main() {}\n");
  execFileSync("cargo", ["generate-lockfile", "--offline"], { cwd: repository, env, stdio: "pipe" });
  writeFileSync(join(bin, "pnpm"), `#!${process.execPath}\nconst fs = require('node:fs');\nif (fs.existsSync('fail-check')) process.exit(1);\n`);
  chmodSync(join(bin, "pnpm"), 0o755);
  git("init", "--bare", remote);
  git("init", "-b", "main");
  git("config", "user.name", "Release Test");
  git("config", "user.email", "release@example.invalid");
  git("add", ".");
  git("commit", "-m", "Initial version");
  git("remote", "add", "origin", remote);
  git("push", "--set-upstream", "origin", "main");
  const runRelease = (increment = "patch") => spawnSync(process.execPath, [releaseScript, increment], {
    cwd: repository, env, encoding: "utf8",
  });
  return { repository, remote, git, write, runRelease };
}

test("increments stable versions and resets lower components", () => {
  assert.equal(bumpVersion(INITIAL_VERSION, "patch"), "0.6.6");
  assert.equal(bumpVersion(INITIAL_VERSION, "minor"), "0.7.0");
  assert.equal(bumpVersion(INITIAL_VERSION, "major"), "1.0.0");
  assert.equal(bumpVersion("1.9.9", "patch"), "1.9.10");
  assert.throws(() => bumpVersion(INITIAL_VERSION, "other"), /Usage/);
  assert.throws(() => bumpVersion("1.0.0-beta.1", "patch"), /stable/);
  assert.throws(() => bumpVersion("01.0.0", "patch"), /stable/);
});

test("synchronizes all versions, commits, and pushes the branch and tag", () => {
  const { repository, git, runRelease } = createRepository();
  const result = runRelease();
  assert.equal(result.status, 0, result.stderr);
  for (const path of ["package.json", "src-tauri/tauri.conf.json"]) {
    assert.equal(JSON.parse(readFileSync(join(repository, path), "utf8")).version, "0.6.6");
  }
  for (const path of ["src-tauri/Cargo.toml", "Cargo.lock"]) {
    assert.match(readFileSync(join(repository, path), "utf8"), /version = "0\.6\.6"/);
  }
  assert.equal(git("status", "--porcelain"), "");
  assert.equal(git("log", "-1", "--format=%s"), `chore: release ${NEXT_TAG}`);
  const head = git("rev-parse", "HEAD");
  assert.equal(git("rev-parse", NEXT_TAG), head);
  assert.equal(git("ls-remote", "origin", "refs/heads/main"), `${head}\trefs/heads/main`);
  assert.equal(git("ls-remote", "origin", `refs/tags/${NEXT_TAG}`), `${head}\trefs/tags/${NEXT_TAG}`);
});

test("refuses a dirty working tree without changing versions", () => {
  const { repository, write, runRelease } = createRepository();
  write("uncommitted.txt", "unfinished work");
  const result = runRelease();
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Commit or stash/);
  assert.equal(JSON.parse(readFileSync(join(repository, "package.json"), "utf8")).version, INITIAL_VERSION);
});

test("refuses mismatched versions and existing tags", () => {
  const { git, write, runRelease } = createRepository();
  write("src-tauri/tauri.conf.json", '{"version":"0.6.4"}');
  assert.match(runRelease().stderr, /versions must match/);
  write("src-tauri/tauri.conf.json", JSON.stringify({ version: INITIAL_VERSION }));
  git("tag", NEXT_TAG);
  assert.match(runRelease().stderr, /already exists/);
});

test("a failed check leaves reviewable version edits without committing or pushing", () => {
  const { git, write, runRelease } = createRepository();
  write("fail-check", "");
  git("add", "fail-check");
  git("commit", "-m", "Fail checks");
  const head = git("rev-parse", "HEAD");
  const remoteBefore = git("ls-remote", "origin", "refs/heads/main");
  const result = runRelease();
  assert.equal(result.status, 1);
  assert.match(result.stderr, /pnpm check/);
  assert.equal(git("rev-parse", "HEAD"), head);
  assert.equal(git("tag", "--list", NEXT_TAG), "");
  assert.match(git("diff", "--", "package.json"), /0\.6\.6/);
  assert.equal(git("ls-remote", "origin", "refs/heads/main"), remoteBefore);
});

test("an atomic push rejection leaves both remote branch and release tag untouched", () => {
  const { remote, git, runRelease } = createRepository();
  const hook = join(remote, "hooks", "update");
  writeFileSync(hook, '#!/bin/sh\ncase "$1" in refs/tags/*) exit 1;; esac\n');
  chmodSync(hook, 0o755);
  const remoteBefore = git("ls-remote", "origin", "refs/heads/main");
  const result = runRelease();
  assert.equal(result.status, 1);
  assert.match(result.stderr, /atomic/);
  assert.equal(git("ls-remote", "origin", "refs/heads/main"), remoteBefore);
  assert.equal(git("ls-remote", "origin", `refs/tags/${NEXT_TAG}`), "");
  assert.equal(git("rev-parse", NEXT_TAG), git("rev-parse", "HEAD"));
});
