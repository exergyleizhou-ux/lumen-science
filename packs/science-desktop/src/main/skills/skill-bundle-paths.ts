// Adapted from Open Science at fd2853f0b9bdb6c063ccc1e741687584ab94bf9a.
// A directly importable Skill root may be at the archive root or under at most two wrapper
// directories. Keep this predicate shared by full discovery and future bounded ZIP sniffing so a
// preview never advertises a package whose import will later contain no candidates.
const skillManifestRootPath = (path: string): string | undefined => {
  const segments = path.split('/')
  if (segments.length > 3 || segments[segments.length - 1].toLowerCase() !== 'skill.md') {
    return undefined
  }
  return segments.slice(0, -1).join('/')
}

const isSkillManifestPath = (path: string): boolean => skillManifestRootPath(path) !== undefined

// Keeps only the shallowest manifest root on each branch. A nested SKILL.md beneath another root is
// that outer Skill's resource, not a second candidate — even when the outer manifest later proves
// invalid.
const selectSkillManifestRoots = (paths: Iterable<string>): string[] => {
  const candidates = new Set<string>()
  for (const path of paths) {
    const root = skillManifestRootPath(path)
    if (root !== undefined) candidates.add(root)
  }

  return [...candidates]
    .filter((root) => {
      if (root === '') return true
      const segments = root.split('/')
      for (let length = 0; length < segments.length; length += 1) {
        if (candidates.has(segments.slice(0, length).join('/'))) return false
      }
      return true
    })
    .sort((left, right) => left.localeCompare(right))
}

export { isSkillManifestPath, selectSkillManifestRoots, skillManifestRootPath }
