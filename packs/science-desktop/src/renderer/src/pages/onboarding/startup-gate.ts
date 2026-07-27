// Modified from Open Science (Apache-2.0).
// Upstream: https://github.com/aipoch/open-science @ d8f11e34314f
// Change: the gate now considers whether the Lumen engine bridge is present,
// because the upstream wizard configures four subsystems this product does not
// have. Per-file diff and digests: docs/provenance/open-science-adoption.json

// Pure startup decision.
//
// Upstream ran an explicit first-time setup even when the machine already
// satisfied every dependency: environment provisioning, agent-framework choice,
// notebook runtime, and data-root location.
//
// Four of those five steps configure subsystems Lumen deliberately does not
// have. Environments are provisioned by the engine, not by Electron
// (`provisioner.ts` was explicitly not adopted — it is a decision module that
// boots itself). Agent frameworks are stubbed: no Claude Code / Codex /
// OpenCode backend is admitted as a peer authority. The data root is superseded
// by the engine's store roots.
//
// So on a Lumen build the wizard asked the user to configure things that do not
// exist, and blocked entry to the product until they did. A setup flow for
// absent capabilities is worse than none: it cannot succeed, and it makes the
// product look broken rather than deliberately narrower.
//
// The gate therefore asks the question that IS meaningful here — is this a
// build whose engine owns those concerns? — instead of the four that are not.

export type StartupView = 'onboarding' | 'app'

export type StartupGateInput = {
  onboardingDone: boolean
  /**
   * Whether the Lumen engine bridge is exposed to the renderer.
   *
   * Present means the engine owns environments, storage and execution, so there
   * is nothing for the upstream wizard to configure. Absent means the legacy
   * Open Science path, where that wizard still applies.
   *
   * Optional, and omitted is treated as absent: that preserves upstream
   * behaviour for a build that still needs setup, rather than silently skipping
   * it. Skipping setup should require positive evidence, not a missing field.
   */
  lumenBridgePresent?: boolean
}

export const resolveStartupView = (input: StartupGateInput): StartupView => {
  // A Lumen build goes straight to the product — not because setup succeeded,
  // but because none of what that setup configures exists here. Asking anyway
  // is how the first screen a user saw ended up being someone else's
  // onboarding wizard.
  if (input.lumenBridgePresent) return 'app'
  return input.onboardingDone ? 'app' : 'onboarding'
}
