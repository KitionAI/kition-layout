/**
 * Presses in a row (plan 2026-11-25, stage 5): the Kit tools' pointer event carries no click
 * count, so the editor counts presses that follow each other closely in time and place and tells
 * the engine the count of the next press (`setClickCount`): two select a word with the Type tool,
 * three a line, four a paragraph, five the story. Pure, so it is tested in Node.
 */

/** The longest pause between two presses of one multiple click, in milliseconds. */
export const CLICK_INTERVAL_MS = 500
/** How far (CSS pixels) the pointer may move between two presses of one multiple click. */
export const CLICK_SLOP_PX = 4

export type Press = { time: number; x: number; y: number; count: number }

/** The press at `time` and (`x`, `y`) after the one before (`last`, or null): its count. */
export function nextPress(last: Press | null, time: number, x: number, y: number): Press {
  const near = last !== null && time - last.time >= 0 && time - last.time <= CLICK_INTERVAL_MS && Math.hypot(x - last.x, y - last.y) <= CLICK_SLOP_PX
  return { time, x, y, count: near && last ? Math.min(last.count + 1, 255) : 1 }
}

/** A counted press waiting for the editor to send its press on (plan 2026-11-25, stage 6). */
export type Counted = { count: number; at: number }

/** The longest a counted press waits for its own press to be sent (a press the view tools took is never sent). */
export const PRESS_WAIT_MS = 2000

/**
 * The count for the press the editor sends now (at `now`): the oldest counted press still
 * waiting, so a press sent after the next one was counted (a fast double click) keeps its own
 * count; presses waiting too long are dropped first. One when none waits. Changes `waiting`.
 */
export function takeCount(waiting: Counted[], now: number): number {
  while (waiting.length && now - (waiting[0]?.at ?? now) > PRESS_WAIT_MS) waiting.shift()
  return waiting.shift()?.count ?? 1
}
