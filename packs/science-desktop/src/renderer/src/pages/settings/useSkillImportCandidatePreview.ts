import { useCallback, useRef, useState } from 'react'

import type { SkillImportPreviewContent } from '../../../../shared/settings'

type SkillImportCandidatePreviewState = {
  open: boolean
  onOpenChange: (open: boolean) => void
  loading: boolean
  error: string | null
  content: SkillImportPreviewContent | null
}

type SkillImportCandidatePreviewController = {
  openPreview: (load: () => SkillImportPreviewContent | Promise<SkillImportPreviewContent>) => void
  invalidatePreview: () => void
  previewProps: SkillImportCandidatePreviewState
}

const previewErrorMessage = (error: unknown): string => {
  const message = error instanceof Error ? error.message : String(error)
  return message.replace(/^Error invoking remote method '[^']*':\s*/, '').replace(/^Error:\s*/, '')
}

const isPromiseLike = (
  value: SkillImportPreviewContent | Promise<SkillImportPreviewContent>
): value is Promise<SkillImportPreviewContent> =>
  typeof (value as Promise<SkillImportPreviewContent>).then === 'function'

// Adapted from Open Science at fd2853f0b9bdb6c063ccc1e741687584ab94bf9a.
// A close or a newer row click invalidates an in-flight result, so late IPC/network responses cannot
// reopen or replace the dialog.
const useSkillImportCandidatePreview = (): SkillImportCandidatePreviewController => {
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [content, setContent] = useState<SkillImportPreviewContent | null>(null)
  const generation = useRef(0)

  const closePreview = useCallback((): void => {
    generation.current += 1
    setOpen(false)
  }, [])

  const invalidatePreview = useCallback((): void => {
    generation.current += 1
    setOpen(false)
    setLoading(false)
    setError(null)
    setContent(null)
  }, [])

  const onOpenChange = useCallback(
    (nextOpen: boolean): void => {
      if (nextOpen) setOpen(true)
      else closePreview()
    },
    [closePreview]
  )

  const openPreview = useCallback(
    (load: () => SkillImportPreviewContent | Promise<SkillImportPreviewContent>): void => {
      const request = generation.current + 1
      generation.current = request
      setError(null)

      try {
        const result = load()
        if (!isPromiseLike(result)) {
          setContent(result)
          setLoading(false)
          setOpen(true)
          return
        }

        setContent(null)
        setLoading(true)
        setOpen(true)
        void Promise.resolve(result)
          .then((nextContent) => {
            if (generation.current === request) setContent(nextContent)
          })
          .catch((reason) => {
            if (generation.current === request) setError(previewErrorMessage(reason))
          })
          .finally(() => {
            if (generation.current === request) setLoading(false)
          })
      } catch (reason) {
        setContent(null)
        setLoading(false)
        setError(previewErrorMessage(reason))
        setOpen(true)
      }
    },
    []
  )

  return {
    openPreview,
    invalidatePreview,
    previewProps: { open, onOpenChange, loading, error, content }
  }
}

export { useSkillImportCandidatePreview }
