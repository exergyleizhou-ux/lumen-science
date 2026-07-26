import { ExternalLink, FolderOpen, Globe, Terminal } from 'lucide-react'
import { useEffect, useState } from 'react'

import { ExternalTextLink } from '@/components/ExternalTextLink'
import { GitHubStarBadge } from '@/components/GitHubStarBadge'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import { useSettingsStore } from '@/stores/settings-store'
import type { CloseActionPreference } from '../../../../shared/window-controls'
import type { CliLauncherStatus } from '../../../../shared/cli'
import { APP } from '../../../../shared/app-config'
import { AppIconSection } from './AppIconSection'
import { AppVersionSection } from './AppVersionSection'
import { SettingsRow, SettingsSection, SettingsToggle } from './SettingsLayout'

// Community entry links (Discord, X) share the GitHub badge's compact look so the row reads as one
// set of "connect with the project" actions.
const socialLinkClassName =
  'inline-flex h-8 items-center gap-1.5 rounded-lg border border-border bg-card px-2 text-xs font-medium text-muted-foreground transition-colors duration-150 motion-reduce:transition-none hover:bg-muted hover:text-foreground'

// General app settings. Hosts the Diagnostics (log file) tools and the community/connect links. The log
// file stays on this device and is never transmitted by the app.
const GeneralPanel = (): React.JSX.Element => {
  const [logPath, setLogPath] = useState<string | null>(null)
  const [message, setMessage] = useState<string | undefined>(undefined)
  const [isOpening, setIsOpening] = useState(false)
  const [cli, setCli] = useState<CliLauncherStatus | null>(null)
  const [isUpdatingCli, setIsUpdatingCli] = useState(false)
  const [cliError, setCliError] = useState<string | undefined>(undefined)
  const notificationsEnabled = useSettingsStore((state) => state.notificationsEnabled)
  const setNotificationsEnabled = useSettingsStore((state) => state.setNotificationsEnabled)
  const closePreference = useSettingsStore((state) => state.closePreference)
  const setClosePreference = useSettingsStore((state) => state.setClosePreference)

  useEffect(() => {
    void window.api.logs.getPath().then(setLogPath)
    void window.api.cli.getStatus().then(setCli)
  }, [])

  const handleCli = async (action: 'install' | 'uninstall'): Promise<void> => {
    setIsUpdatingCli(true)
    setCliError(undefined)

    try {
      setCli(
        action === 'install' ? await window.api.cli.install() : await window.api.cli.uninstall()
      )
    } catch (error) {
      setCliError(
        error instanceof Error ? error.message : 'Could not update the command-line tool.'
      )
    } finally {
      setIsUpdatingCli(false)
    }
  }

  const handleOpenLog = async (): Promise<void> => {
    setIsOpening(true)
    setMessage(undefined)

    try {
      const result = await window.api.logs.openFile()

      if (!result.opened) {
        setMessage(result.error ?? 'Could not open the log file.')
      }
    } catch (error) {
      setMessage(error instanceof Error ? error.message : 'Could not open the log file.')
    } finally {
      setIsOpening(false)
    }
  }

  const handleReveal = async (): Promise<void> => {
    setMessage(undefined)

    try {
      const result = await window.api.logs.revealInFolder()

      if (!result.revealed) {
        setMessage(result.error ?? 'Could not reveal the log file.')
      }
    } catch (error) {
      setMessage(error instanceof Error ? error.message : 'Could not reveal the log file.')
    }
  }

  return (
    <div className="space-y-5 p-5">
      <AppVersionSection />

      {window.api.platform === 'win32' && window.api.window?.onCloseConfirmRequest ? (
        <SettingsSection
          title="Window behavior"
          description="Choose what the titlebar close button does."
          aria-label="Window behavior"
          separated
        >
          <SettingsRow
            label="When closing the window"
            description="Ask each time, keep Lumen Science running in the tray, or quit the app."
            className="pt-0"
          >
            <Select
              value={closePreference ?? 'ask'}
              onValueChange={(value) =>
                void setClosePreference(
                  value === 'ask' ? undefined : (value as CloseActionPreference)
                )
              }
            >
              <SelectTrigger aria-label="When closing the window">
                <span>
                  {closePreference === 'minimize'
                    ? 'Minimize to tray'
                    : closePreference === 'quit'
                      ? 'Quit'
                      : 'Ask every time'}
                </span>
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="ask">Ask every time</SelectItem>
                <SelectItem value="minimize">Minimize to tray</SelectItem>
                <SelectItem value="quit">Quit</SelectItem>
              </SelectContent>
            </Select>
          </SettingsRow>
        </SettingsSection>
      ) : null}

      <SettingsSection
        title="Notifications"
        description={
          <>
            Get a desktop notification when a task finishes, fails, or waits for your approval while
            you&apos;re away from the app.
          </>
        }
        aria-label="Notifications"
        separated
      >
        <SettingsRow
          label="Task notifications"
          description="Selecting a notification brings Lumen Science back to the front and opens the task."
          className="pt-0"
        >
          <div className="flex justify-end">
            <SettingsToggle
              enabled={notificationsEnabled}
              aria-label="Toggle task notifications"
              onToggle={() => void setNotificationsEnabled(!notificationsEnabled)}
            />
          </div>
        </SettingsRow>

        <p className="mt-1 text-xs text-muted-foreground">
          Notifications only appear while you&apos;re using another app. Tasks you cancel and
          failures the app retries automatically stay silent. Your operating system may ask for
          notification permission the first time one appears.
        </p>
      </SettingsSection>

      <AppIconSection />

      <SettingsSection
        title="Diagnostics"
        description={
          <>
            View this device&apos;s runtime log — it records what the app is doing so problems can
            be diagnosed.
          </>
        }
        aria-label="Diagnostics"
        separated
      >
        <SettingsRow label="Log file" controlClassName="w-auto justify-self-end" className="pt-0">
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => void handleReveal()}
              disabled={!logPath}
            >
              <FolderOpen className="size-4" aria-hidden="true" />
              Reveal
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => void handleOpenLog()}
              disabled={isOpening || !logPath}
            >
              <ExternalLink className="size-4" aria-hidden="true" />
              {isOpening ? 'Opening…' : 'Open'}
            </Button>
          </div>
        </SettingsRow>

        <pre
          className="overflow-x-auto rounded-lg border border-border bg-muted/60 px-3 py-2.5 font-mono text-xs text-foreground"
          aria-label="Log file path"
        >
          {logPath ?? 'Not available yet.'}
        </pre>

        {message ? (
          <p className="mt-2 text-xs text-destructive" role="alert">
            {message}
          </p>
        ) : null}

        <p className="mt-3 text-xs text-muted-foreground">
          Something not working?{' '}
          <ExternalTextLink href={APP.links.githubIssues}>Open an issue on GitHub</ExternalTextLink>{' '}
          and attach the log above. It stays on this device and is never sent automatically; it may
          contain local file paths, so review it before sharing.
        </p>
      </SettingsSection>

      <SettingsSection
        title="Command line tool"
        description={
          <>
            Install the <code className="font-mono">open-science</code> command so you can start,
            stop, and check the backend from a terminal, then use it entirely from your browser.
          </>
        }
        aria-label="Command line tool"
        separated
      >
        <SettingsRow
          label="open-science"
          controlClassName="w-auto justify-self-end"
          className="pt-0"
        >
          <Button
            type="button"
            variant="outline"
            onClick={() => void handleCli(cli?.installed ? 'uninstall' : 'install')}
            disabled={isUpdatingCli || cli === null}
          >
            <Terminal className="size-4" aria-hidden="true" />
            {isUpdatingCli ? 'Working…' : cli?.installed ? 'Uninstall command' : 'Install command'}
          </Button>
        </SettingsRow>

        {cli?.installed ? (
          <pre
            className="overflow-x-auto rounded-lg border border-border bg-muted/60 px-3 py-2.5 font-mono text-xs text-foreground"
            aria-label="Command line tool path"
          >
            {cli.target}
          </pre>
        ) : null}

        {cli?.installed && cli.pathHint ? (
          <p className="mt-2 text-xs text-muted-foreground">{cli.pathHint}</p>
        ) : null}

        {cliError ? (
          <p className="mt-2 text-xs text-destructive" role="alert">
            {cliError}
          </p>
        ) : null}

        <p className="mt-3 text-xs text-muted-foreground">
          Once installed, run <code className="font-mono">open-science start</code> to launch the
          backend and open the authenticated URL, then{' '}
          <code className="font-mono">open-science stop</code> to shut it down.{' '}
          <code className="font-mono">status</code> and <code className="font-mono">url</code> are
          also available.
        </p>
      </SettingsSection>

      <SettingsSection
        title="Enjoying Lumen Science?"
        description={
          <>
            It&apos;s free and open source. Star it on GitHub to help others find it, and open an
            issue if something is wrong. Thanks for being here.
          </>
        }
        aria-label="Community"
        separated
      >
        <div className="flex flex-wrap items-center gap-2">
          <GitHubStarBadge className="border border-border" />
          {/* LS5-R1-02: the inherited Discord and X links pointed at the upstream
              Open Science project's community. Sending Lumen Science users there
              under Lumen branding misrepresents both projects, so they are gone
              rather than re-pointed — Lumen has no equivalent channels yet. */}
          <a
            href={APP.links.website}
            target="_blank"
            rel="noreferrer"
            aria-label={`Open the ${APP.name} website`}
            className={socialLinkClassName}
          >
            <Globe className="size-4" strokeWidth={2} aria-hidden="true" />
            Website
          </a>
        </div>
      </SettingsSection>
    </div>
  )
}

export { GeneralPanel }
