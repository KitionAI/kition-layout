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
