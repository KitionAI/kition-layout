/**
 * The engine's errors in the person's language (plan 2026-11-25, stage 8): no English engine text
 * reaches a toast or a dialog. An engine error says its kind and command in a fixed form
 * (`invalid parameters for \`cmd\`: …`, `command \`cmd\` is not available right now: …`,
 * `unknown command \`cmd\``, `no active document`, `internal error in \`cmd\` …`), and a refusal a
 * person can meet carries a stable code at its end (`[refusal:needTwo]`). The editor words the
 * error by its code, else by its kind and command; the English rest is never shown. Pure, so it is
 * tested in Node.
 */

/** The kinds of engine errors. */
export type ErrorKind = 'unknown' | 'disabled' | 'badParams' | 'noDocument' | 'internal' | 'other'

/** An engine error as the editor reads it: its kind, the command it names, and its refusal code. */
export type EngineError = { kind: ErrorKind; command: string | null; code: string | null }

const TEMPLATES: ReadonlyArray<readonly [RegExp, ErrorKind]> = [
  [/invalid parameters for `([^`]+)`/, 'badParams'],
  [/command `([^`]+)` is not available right now/, 'disabled'],
  [/unknown command `([^`]+)`/, 'unknown'],
  [/internal error in `([^`]+)`/, 'internal'],
  // The binding refuses a parameter a command does not take, and an address that names nothing.
  [/`([^`]+)` has no parameter `/, 'badParams'],
  [/no active document/, 'noDocument'],
]

/** The text of an error, whatever was thrown. */
function textOf(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

/** Reads an engine error's kind, command and refusal code. */
export function parseEngineError(error: unknown): EngineError {
  const text = textOf(error)
  const code = /\[refusal:([A-Za-z]+)\]/.exec(text)?.[1] ?? null
  for (const [pattern, kind] of TEMPLATES) {
    const match = pattern.exec(text)
    if (match) return { kind, command: match[1] ?? null, code }
  }
  return { kind: 'other', command: null, code }
}

/**
 * A refusal by its kind and command where the command has one refusal only (an error without a
 * code): Transform Again with nothing to repeat.
 */
const BY_COMMAND: Readonly<Record<string, string>> = {
  'transform.again': 'needTransform',
}

/** The words for an error, in the editor's language (`t`); `title` names a command for the person. */
export type Words = { t: (key: string, values?: Record<string, string | number>) => string; title: (command: string) => string; reasons: ReadonlySet<string> }

/** The reason key (`hint.<reason>`) an error's code or kind and command stand for, or null. */
export function reasonOf(error: unknown, reasons: ReadonlySet<string>): string | null {
  const parsed = parseEngineError(error)
  if (parsed.code && reasons.has(parsed.code)) return parsed.code
  if (parsed.kind === 'badParams' && parsed.command) return BY_COMMAND[parsed.command] ?? null
  return null
}

/** What to tell the person about an engine error: never the engine's English. */
export function errorText(error: unknown, words: Words): string {
  const reason = reasonOf(error, words.reasons)
  if (reason) return words.t(`hint.${reason}`)
  const { kind, command } = parseEngineError(error)
  const title = command ? words.title(command) : ''
  switch (kind) {
    case 'unknown':
      return words.t('error.unknown')
    case 'disabled':
      return title ? words.t('error.disabled', { title }) : words.t('hint.notNow')
    case 'badParams':
      return title ? words.t('error.badParams', { title }) : words.t('error.other')
    case 'noDocument':
      return words.t('error.noDocument')
    case 'internal':
      return title ? words.t('error.internal', { title }) : words.t('error.other')
    default:
      return words.t('error.other')
  }
}
