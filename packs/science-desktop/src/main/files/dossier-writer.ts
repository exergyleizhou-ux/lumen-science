/**
 * Writes a dossier to disk as something a third party can actually check.
 *
 * `buildDossierPackage` returns a map of filenames to JSON strings and nothing
 * else — no artifact bytes, and no caller that writes it anywhere. So until now
 * there was no export path at all: the evidence existed inside the app and
 * could not leave it in a form anyone could verify.
 *
 * What a dossier has to contain to be worth anything:
 *
 *   1. the artifact BYTES, not just their digests. A manifest listing
 *      `{artifactId, sha256}` asserts things about files nobody has. Re-hashing
 *      is the whole verification, and it needs the bytes.
 *   2. the verifier itself, so a reader does not have to obtain it from the
 *      party whose claims they are checking.
 *
 * Every digest is re-computed here from the bytes about to be written and
 * compared to what the manifest claims. A mismatch aborts the export: shipping
 * a package that fails verification would be worse than shipping none, because
 * the failure would look like tampering in transit rather than a bug at the
 * source.
 */

import { createHash } from 'node:crypto'
import { copyFile, mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'

/**
 * The document set to write: filename → contents.
 *
 * Deliberately structural rather than importing `DossierPackageFiles`. The
 * writer's job is to lay bytes on disk verifiably; it has no reason to know
 * which builder produced them, and coupling to that builder would drag an
 * otherwise-unreachable module into the production graph.
 */
export type DossierDocuments = Readonly<Record<string, string>>

export type ArtifactSource = {
  artifactId: string
  sha256: string
  /** Absolute path to the bytes, as resolved by the preview store. */
  path: string
}

export type DossierWriteResult = {
  directory: string
  filesWritten: string[]
  artifactsWritten: number
  /** Artifacts whose bytes were unavailable, so a reader is told what is missing. */
  artifactsOmitted: { artifactId: string; reason: string }[]
  verifierIncluded: boolean
}

export type DossierWriteDeps = {
  /** Reads one artifact's bytes. Injected so this stays testable without a store. */
  readArtifact: (source: ArtifactSource) => Promise<Buffer>
  /** Absolute path to scripts/verify-dossier.py, if it should travel with the package. */
  verifierPath?: string
}

const sha256 = (bytes: Buffer): string => createHash('sha256').update(bytes).digest('hex')

const CANONICAL_DIGEST = /^[0-9a-f]{64}$/

/**
 * Write a complete, independently verifiable dossier directory.
 *
 * Throws rather than emitting a package whose digests do not match its bytes.
 */
export async function writeDossier(
  directory: string,
  pkg: { files: DossierDocuments },
  artifacts: ArtifactSource[],
  deps: DossierWriteDeps
): Promise<DossierWriteResult> {
  await mkdir(path.join(directory, 'artifacts'), { recursive: true })

  const filesWritten: string[] = []
  for (const [name, body] of Object.entries(pkg.files)) {
    const target = path.join(directory, name)
    await mkdir(path.dirname(target), { recursive: true })
    await writeFile(target, body, 'utf8')
    filesWritten.push(name)
  }

  const artifactsOmitted: { artifactId: string; reason: string }[] = []
  let artifactsWritten = 0

  for (const source of artifacts) {
    // A digest that is not canonical 64-hex is refused before it names a path.
    // Truncation is how two distinct artifacts collapse onto one identity, and
    // this value is used as a filename.
    if (!CANONICAL_DIGEST.test(source.sha256)) {
      artifactsOmitted.push({
        artifactId: source.artifactId,
        reason: `digest is not canonical 64-hex: ${source.sha256}`
      })
      continue
    }

    let bytes: Buffer
    try {
      bytes = await deps.readArtifact(source)
    } catch (error: unknown) {
      // Missing bytes are recorded, not fatal: an artifact may legitimately be
      // too large or no longer present. The verifier reports the gap, and fails
      // only if NOTHING shipped.
      artifactsOmitted.push({
        artifactId: source.artifactId,
        reason: (error as Error)?.message || String(error)
      })
      continue
    }

    const actual = sha256(bytes)
    if (actual !== source.sha256) {
      // Abort. The manifest already claims this digest, so writing these bytes
      // would produce a package that fails verification — and that failure
      // would read as tampering in transit rather than a bug here.
      throw new Error(
        `dossier export aborted: artifact ${source.artifactId} hashes to ${actual}, ` +
          `manifest claims ${source.sha256}`
      )
    }

    await writeFile(path.join(directory, 'artifacts', source.sha256), bytes)
    artifactsWritten += 1
  }

  // Ship the verifier inside the package. A reader who has to ask us for the
  // tool that checks our claims is still trusting us.
  let verifierIncluded = false
  if (deps.verifierPath) {
    try {
      await copyFile(deps.verifierPath, path.join(directory, 'verify-dossier.py'))
      verifierIncluded = true
    } catch {
      // Non-fatal, and reported: the dossier is still verifiable by someone who
      // fetches the verifier separately.
      verifierIncluded = false
    }
  }

  if (artifactsOmitted.length > 0) {
    await writeFile(
      path.join(directory, 'artifacts', 'OMITTED.json'),
      JSON.stringify({ omitted: artifactsOmitted }, null, 2) + '\n',
      'utf8'
    )
  }

  return {
    directory,
    filesWritten,
    artifactsWritten,
    artifactsOmitted,
    verifierIncluded
  }
}
