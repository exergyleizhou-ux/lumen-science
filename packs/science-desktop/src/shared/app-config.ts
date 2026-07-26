// Single source of truth for project identity and external links. Shared by the main process
// (GitHub star-count fetch) and the renderer (every entry-point link). Keep this UI-free — no
// icons, no JSX — so both processes can import it and any screen reuses the same values.
//
// LS5-R1-02: this file previously carried the upstream Open Science identity
// (github.com/aipoch/open-science, aipoch.com, their Discord and X accounts)
// and their update feed (statics.aipoch.com). All of it is removed.
//
// Two separate reasons, both disqualifying on their own:
//   - Identity: shipping Lumen Science under another project's name, copyright
//     and community links misrepresents both projects.
//   - Code delivery: an inherited update feed lets a third party serve
//     executable code to Lumen users.
//
// Update configuration now lives in ./update-policy.ts and is off by default.
// There is deliberately no feed URL in this file for anything to inherit.

const GITHUB_OWNER = 'exergyleizhou-ux'
const GITHUB_REPO = 'lumen-science'
const GITHUB_REPO_URL = `https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}`

export const APP = {
  name: 'Lumen Science',
  githubOwner: GITHUB_OWNER,
  githubRepo: GITHUB_REPO,
  links: {
    website: GITHUB_REPO_URL,
    githubRepo: GITHUB_REPO_URL,
    githubReleases: `${GITHUB_REPO_URL}/releases`,
    githubApi: `https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}`,
    githubIssues: `${GITHUB_REPO_URL}/issues`
  },
  copyright: '© 2026 Lumen Science contributors. Apache-2.0.',
  update: {
    // A page the user opens by clicking, not an endpoint the app contacts on
    // its own. The automatic path is governed by resolveUpdatePolicy() and
    // stays disabled unless Lumen-owned signing material is configured.
    downloadPage: `${GITHUB_REPO_URL}/releases`
  }
} as const
