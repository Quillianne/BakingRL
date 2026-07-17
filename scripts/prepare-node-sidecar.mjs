import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { constants, createReadStream, createWriteStream } from "node:fs";
import {
  access,
  chmod,
  copyFile,
  mkdir,
  mkdtemp,
  realpath,
  rename,
  rm,
  stat
} from "node:fs/promises";
import path from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

const BUNDLED_NODE_VERSION = "24.18.0";
const OFFICIAL_NODE_BASE_URL = `https://nodejs.org/dist/v${BUNDLED_NODE_VERSION}`;
const DARWIN_DISTRIBUTIONS = {
  "aarch64-apple-darwin": {
    archive: `node-v${BUNDLED_NODE_VERSION}-darwin-arm64.tar.gz`,
    binarySha256: "ee6fb0e015284d83a91e8ec5213f43a157f8a392b58555301682892ba928c04a",
    sha256: "e1a97e14c99c803e96c7339403282ea05a499c32f8d83defe9ef5ec66f979ed1"
  },
  "x86_64-apple-darwin": {
    archive: `node-v${BUNDLED_NODE_VERSION}-darwin-x64.tar.gz`,
    binarySha256: "c5afe80c9fd47c0e1ba3a7221173d061dae04577acc67e21e945d16e34c696c8",
    sha256: "dfd0dbd3e721503434df7b7205e719f61b3a3a31b2bcf9729b8b91fea240f080"
  }
};

const explicitSource = process.env.BAKINGRL_NODE_BINARY;
const triple = process.env.TAURI_ENV_TARGET_TRIPLE || process.env.TARGET || defaultTriple();
const suffix = process.platform === "win32" ? ".exe" : "";
const outDir = path.resolve("src-tauri", "bin");
const destination = path.join(outDir, `node-${triple}${suffix}`);
const preparedSource = explicitSource
  ? await localNodeSource(explicitSource, process.platform === "darwin")
  : process.platform === "darwin"
    ? await officialDarwinNode(triple)
    : await localNodeSource(process.execPath, false);
const expectedVersion = !explicitSource && process.platform === "darwin"
  ? `v${BUNDLED_NODE_VERSION}`
  : undefined;
const source = preparedSource.binary;

await access(source, constants.X_OK);
await mkdir(outDir, { recursive: true });
await copyFile(source, destination);
await chmod(destination, 0o755);
if (process.platform === "darwin") {
  await stageNodeLicense(preparedSource.license);
}

const probe = probeNode(destination);
if (!probe.ok || (expectedVersion && probe.version !== expectedVersion)) {
  await rm(destination, { force: true });
  const versionDetails = expectedVersion && probe.version !== expectedVersion
    ? `Expected ${expectedVersion}, got ${probe.version || "no version"}.`
    : undefined;
  throw new Error(
    `Copied Node binary '${source}' cannot run from the Tauri sidecar location. ` +
      "Use a self-contained Node distribution or set BAKINGRL_NODE_BINARY to one before building." +
      ([versionDetails, probe.details].filter(Boolean).length > 0
        ? `\n${[versionDetails, probe.details].filter(Boolean).join("\n")}`
        : "")
  );
}

console.log(`Prepared Node sidecar: ${destination} (${probe.version})`);

async function officialDarwinNode(targetTriple) {
  const distribution = DARWIN_DISTRIBUTIONS[targetTriple];
  if (!distribution) {
    throw new Error(
      `Unsupported macOS Node sidecar target '${targetTriple}'. Set BAKINGRL_NODE_BINARY ` +
        "to a self-contained Node binary for this target."
    );
  }

  const cacheDir = path.resolve(
    "node_modules",
    ".cache",
    "bakingrl-node",
    `v${BUNDLED_NODE_VERSION}`
  );
  const archivePath = path.join(cacheDir, distribution.archive);
  const cachedBinary = path.join(cacheDir, `${distribution.archive}.node`);
  const cachedLicense = path.join(cacheDir, `${distribution.archive}.LICENSE`);
  await mkdir(cacheDir, { recursive: true });

  if (!(await checksumMatches(cachedBinary, distribution.binarySha256))) {
    await rm(cachedBinary, { force: true });
  }

  if (!(await isFile(cachedBinary)) || !(await isFile(cachedLicense))) {
    if (!(await checksumMatches(archivePath, distribution.sha256))) {
      await rm(archivePath, { force: true });
      await downloadVerifiedArchive(distribution.archive, archivePath, distribution.sha256);
    }
    await extractNodeDistribution(
      distribution,
      archivePath,
      cachedBinary,
      cachedLicense,
      cacheDir
    );
  }

  return {
    binary: await realpath(cachedBinary),
    license: await realpath(cachedLicense)
  };
}

async function downloadVerifiedArchive(archive, archivePath, expectedSha256) {
  const url = `${OFFICIAL_NODE_BASE_URL}/${archive}`;
  const temporaryPath = `${archivePath}.${process.pid}.download`;
  await rm(temporaryPath, { force: true });
  console.log(`Downloading official Node.js v${BUNDLED_NODE_VERSION} runtime from ${url}...`);

  try {
    const response = await fetch(url, { redirect: "follow" });
    if (!response.ok || !response.body) {
      throw new Error(`Download failed with HTTP ${response.status} ${response.statusText}.`);
    }
    await pipeline(
      Readable.fromWeb(response.body),
      createWriteStream(temporaryPath, { mode: 0o600 })
    );
    const actualSha256 = await sha256File(temporaryPath);
    if (actualSha256 !== expectedSha256) {
      throw new Error(
        `Checksum mismatch for '${archive}': expected ${expectedSha256}, got ${actualSha256}.`
      );
    }
    await rename(temporaryPath, archivePath);
  } catch (error) {
    await rm(temporaryPath, { force: true });
    throw error;
  }
}

async function extractNodeDistribution(
  distribution,
  archivePath,
  cachedBinary,
  cachedLicense,
  cacheDir
) {
  const { archive, binarySha256 } = distribution;
  const archiveRoot = archive.replace(/\.tar\.gz$/u, "");
  const binaryMember = `${archiveRoot}/bin/node`;
  const licenseMember = `${archiveRoot}/LICENSE`;
  const extractionRoot = await mkdtemp(path.join(cacheDir, "extract-"));

  try {
    const extraction = spawnSync(
      "tar",
      ["-xzf", archivePath, "-C", extractionRoot, binaryMember, licenseMember],
      { encoding: "utf8", timeout: 30_000 }
    );
    if (extraction.error || extraction.status !== 0) {
      const details = processResultDetails(extraction);
      throw new Error(
        `Unable to extract Node runtime files from '${archive}'.${details ? `\n${details}` : ""}`
      );
    }
    const extractedBinary = path.join(extractionRoot, binaryMember);
    const extractedLicense = path.join(extractionRoot, licenseMember);
    await access(extractedBinary, constants.X_OK);
    const actualBinarySha256 = await sha256File(extractedBinary);
    if (actualBinarySha256 !== binarySha256) {
      throw new Error(
        `Checksum mismatch for Node binary in '${archive}': ` +
          `expected ${binarySha256}, got ${actualBinarySha256}.`
      );
    }
    await copyFile(extractedBinary, cachedBinary);
    await chmod(cachedBinary, 0o755);
    await copyFile(extractedLicense, cachedLicense);
  } finally {
    await rm(extractionRoot, { recursive: true, force: true });
  }
}

async function localNodeSource(binaryPath, requireLicense) {
  const binary = await realpath(binaryPath);
  const configuredLicense = process.env.BAKINGRL_NODE_LICENSE;
  const license = configuredLicense
    ? await realpath(configuredLicense)
    : await firstFile([
        path.join(path.dirname(binary), "LICENSE"),
        path.join(path.dirname(binary), "..", "LICENSE")
      ]);
  if (requireLicense && !license) {
    throw new Error(
      `Unable to find the license for custom Node binary '${binary}'. ` +
        "Set BAKINGRL_NODE_LICENSE to its Node.js LICENSE file."
    );
  }
  return { binary, license };
}

async function stageNodeLicense(license) {
  if (!license) throw new Error("Unable to stage the bundled Node.js license.");
  const resourceDir = path.resolve("src-tauri", "gen", "node");
  await mkdir(resourceDir, { recursive: true });
  await copyFile(license, path.join(resourceDir, "LICENSE"));
}

async function firstFile(candidates) {
  for (const candidate of candidates) {
    if (await isFile(candidate)) return realpath(candidate);
  }
  return undefined;
}

async function checksumMatches(filePath, expectedSha256) {
  if (!(await isFile(filePath))) return false;
  return (await sha256File(filePath)) === expectedSha256;
}

async function sha256File(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) hash.update(chunk);
  return hash.digest("hex");
}

async function isFile(filePath) {
  return stat(filePath).then((value) => value.isFile()).catch(() => false);
}

function probeNode(binary) {
  const probeEnv = { ...process.env };
  delete probeEnv.NODE_OPTIONS;
  delete probeEnv.NODE_PATH;
  const probe = spawnSync(binary, ["--version"], {
    encoding: "utf8",
    env: probeEnv,
    timeout: 10_000,
    windowsHide: true
  });
  return {
    ok: !probe.error && probe.status === 0,
    version: probe.stdout?.trim(),
    details: processResultDetails(probe)
  };
}

function processResultDetails(result) {
  return [
    result.error?.message,
    result.signal ? `signal ${result.signal}` : undefined,
    result.stderr?.trim(),
    result.stdout?.trim()
  ].filter(Boolean).join("\n");
}

function defaultTriple() {
  const key = `${process.platform}:${process.arch}`;
  const triples = {
    "darwin:arm64": "aarch64-apple-darwin",
    "darwin:x64": "x86_64-apple-darwin",
    "linux:arm64": "aarch64-unknown-linux-gnu",
    "linux:x64": "x86_64-unknown-linux-gnu",
    "win32:arm64": "aarch64-pc-windows-msvc",
    "win32:x64": "x86_64-pc-windows-msvc"
  };
  const triple = triples[key];
  if (!triple) {
    throw new Error(`Unsupported Node sidecar target '${key}'. Set TAURI_ENV_TARGET_TRIPLE.`);
  }
  return triple;
}
